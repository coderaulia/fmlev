use std::fs;

/// Checks the Linux Yama ptrace_scope setting.
///
/// Returns the current scope level:
/// 0: Classic ptrace permissions (any process with same UID can ptrace)
/// 1: Restricted ptrace (only parent can ptrace unless CAP_SYS_PTRACE is held)
/// 2: Admin-only ptrace (only root / CAP_SYS_PTRACE can ptrace)
/// 3: No ptrace (all ptrace operations disabled)
pub fn check_ptrace_scope() -> Option<u32> {
    fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Provides human-readable diagnosis and remediation advice for Yama ptrace restrictions.
pub fn get_yama_diagnostic() -> String {
    match check_ptrace_scope() {
        Some(0) => "Yama ptrace_scope is 0 (Permissive). No permission adjustments needed.".to_string(),
        Some(1) => {
            "Yama ptrace_scope is 1 (Restricted).\n\
             To allow fmlev to read Football Manager memory without running as root, run:\n\
             sudo setcap cap_sys_ptrace=eip <path-to-fmlev-binary>\n\
             or temporarily: sudo sysctl -w kernel.yama.ptrace_scope=0".to_string()
        }
        Some(level) => {
            format!(
                "Yama ptrace_scope is {} (Strict). Elevated capabilities or root may be required.",
                level
            )
        }
        None => "Yama LSM not active or inaccessible.".to_string(),
    }
}
