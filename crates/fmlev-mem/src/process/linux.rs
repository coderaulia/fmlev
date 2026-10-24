use std::fs;
use std::path::{Path, PathBuf};
use crate::error::{MemoryError, Result};
use tracing::info;

#[derive(Debug, Clone)]
pub struct GameProcess {
    pub pid: i32,
    pub exe_path: PathBuf,
    pub is_proton: bool,
    pub wine_prefix: Option<PathBuf>,
}

pub fn find_fm_process() -> Result<GameProcess> {
    let proc_dir = fs::read_dir("/proc")?;

    for entry in proc_dir.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if let Ok(pid) = name_str.parse::<i32>() {
            // First check /proc/<pid>/comm for exact executable name
            let comm_path = format!("/proc/{}/comm", pid);
            let comm = fs::read_to_string(&comm_path).unwrap_or_default().trim().to_string();

            if comm.eq_ignore_ascii_case("fm.exe") {
                let cmdline_path = format!("/proc/{}/cmdline", pid);
                let cmdline = fs::read_to_string(&cmdline_path).unwrap_or_default();
                info!("Discovered Football Manager process PID {} (comm: {}, cmd: {})", pid, comm, cmdline);

                let wine_prefix = inspect_wineprefix(pid);
                let exe_link = fs::read_link(format!("/proc/{}/exe", pid)).unwrap_or_else(|_| PathBuf::from("fm.exe"));

                return Ok(GameProcess {
                    pid,
                    exe_path: exe_link,
                    is_proton: true,
                    wine_prefix,
                });
            }
        }
    }

    // Fallback: search cmdline while explicitly filtering out wrapper scripts
    let proc_dir = fs::read_dir("/proc")?;
    for entry in proc_dir.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if let Ok(pid) = name_str.parse::<i32>() {
            let cmdline_path = format!("/proc/{}/cmdline", pid);
            if let Ok(cmdline_bytes) = fs::read(&cmdline_path) {
                let cmdline = String::from_utf8_lossy(&cmdline_bytes).replace('\0', " ");
                let lower_cmd = cmdline.to_lowercase();

                // Exclude known wrapper launchers
                if lower_cmd.contains("systemd-inhibit")
                    || lower_cmd.contains("powerprofilesctl")
                    || lower_cmd.contains("reaper")
                    || lower_cmd.contains("srt-bwrap")
                    || lower_cmd.contains("pv-adverb")
                    || lower_cmd.contains("waitforexitandrun")
                {
                    continue;
                }

                if lower_cmd.contains("fm.exe") {
                    info!("Discovered Football Manager process PID {} via fallback ({})", pid, cmdline);
                    let is_proton = lower_cmd.contains("wine") || Path::new(&format!("/proc/{}/root", pid)).exists();
                    let wine_prefix = inspect_wineprefix(pid);
                    let exe_link = fs::read_link(format!("/proc/{}/exe", pid)).unwrap_or_else(|_| PathBuf::from("fm.exe"));

                    return Ok(GameProcess {
                        pid,
                        exe_path: exe_link,
                        is_proton,
                        wine_prefix,
                    });
                }
            }
        }
    }

    Err(MemoryError::ProcessNotFound)
}

fn inspect_wineprefix(pid: i32) -> Option<PathBuf> {
    let environ_path = format!("/proc/{}/environ", pid);
    if let Ok(environ_bytes) = fs::read(&environ_path) {
        for env_var in environ_bytes.split(|&b| b == 0) {
            let env_str = String::from_utf8_lossy(env_var);
            if let Some(stripped) = env_str.strip_prefix("WINEPREFIX=") {
                return Some(PathBuf::from(stripped));
            }
            if let Some(stripped) = env_str.strip_prefix("STEAM_COMPAT_DATA_PATH=") {
                return Some(PathBuf::from(format!("{}/pfx", stripped)));
            }
        }
    }
    None
}
