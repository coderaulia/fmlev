use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tracing::info;
use crate::error::{OverlayError, Result};

#[derive(Debug, Clone)]
pub struct BrowserCandidate {
    pub name: &'static str,
    pub executable: &'static str,
    pub supports_app_mode: bool,
}

const CANDIDATES: &[BrowserCandidate] = &[
    BrowserCandidate {
        name: "Google Chrome",
        executable: "google-chrome",
        supports_app_mode: true,
    },
    BrowserCandidate {
        name: "Google Chrome Stable",
        executable: "google-chrome-stable",
        supports_app_mode: true,
    },
    BrowserCandidate {
        name: "Chromium",
        executable: "chromium",
        supports_app_mode: true,
    },
    BrowserCandidate {
        name: "Chromium Browser",
        executable: "chromium-browser",
        supports_app_mode: true,
    },
    BrowserCandidate {
        name: "Brave Browser",
        executable: "brave-browser",
        supports_app_mode: true,
    },
    BrowserCandidate {
        name: "Microsoft Edge",
        executable: "microsoft-edge",
        supports_app_mode: true,
    },
    BrowserCandidate {
        name: "Firefox",
        executable: "firefox",
        supports_app_mode: false,
    },
    BrowserCandidate {
        name: "XDG Open (Default System Browser)",
        executable: "xdg-open",
        supports_app_mode: false,
    },
];

pub struct OverlayLauncher;

impl OverlayLauncher {
    /// Detects the best available browser on the system that supports app-mode overlay
    pub fn detect_browser() -> Option<BrowserCandidate> {
        for candidate in CANDIDATES {
            if Self::is_executable_in_path(candidate.executable) {
                return Some(candidate.clone());
            }
        }
        None
    }

    /// Checks if a binary name exists in the current system PATH
    fn is_executable_in_path(executable: &str) -> bool {
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in path_var.split(':') {
                let candidate_path = PathBuf::from(dir).join(executable);
                if candidate_path.is_file() {
                    return true;
                }
            }
        }
        false
    }

    /// Launches an always-on-top compact HUD window via Chromium/Brave/Edge `--app` mode,
    /// or falls back to system browser
    pub fn launch(port: u16) -> Result<Child> {
        let url = format!("http://127.0.0.1:{}?mode=overlay", port);
        info!("Launching Linux HUD Overlay pointing to: {}", url);

        let candidate = Self::detect_browser().ok_or(OverlayError::BrowserNotFound)?;
        info!("Found compatible browser: {} ({})", candidate.name, candidate.executable);

        let mut cmd = Command::new(candidate.executable);

        if candidate.supports_app_mode {
            let user_data_dir = std::env::temp_dir().join("fmlev-overlay-profile");
            cmd.arg(format!("--app={}", url))
                .arg("--window-size=440,750")
                .arg("--window-position=40,40")
                .arg(format!("--user-data-dir={}", user_data_dir.display()))
                .arg("--class=fmlev-overlay")
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-extensions");
        } else if candidate.executable == "firefox" {
            // Firefox kiosk / new-window
            cmd.arg("--new-window").arg(&url);
        } else {
            // Generic xdg-open
            cmd.arg(&url);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            OverlayError::LaunchFailed(format!("Failed to spawn {}: {}", candidate.executable, e))
        })?;

        info!("Successfully launched overlay window (PID: {})", child.id());
        Ok(child)
    }

    /// Prints instructions for configuring Linux Window Managers to keep FMLE overlay always on top
    pub fn window_manager_tips() -> &'static str {
        r#"
======================================================================
  FMLE by Vanaila - ALWAYS ON TOP LINUX HUD OVERLAY GUIDE
======================================================================
The overlay window uses WM_CLASS / app_id: 'fmlev-overlay'.
To pin this window on top of Football Manager 26:

- KDE Plasma:
  Right-click overlay titlebar -> More Actions -> Keep Above Others,
  or System Settings -> Window Rules -> Add Rule for Window Class 'fmlev-overlay' -> Keep Above.

- GNOME / Wayland:
  Right-click titlebar -> 'Always on Top'.

- Hyprland:
  Add to ~/.config/hypr/hyprland.conf:
  windowrule = float, class:^(fmlev-overlay)$
  windowrule = pin, class:^(fmlev-overlay)$
  windowrule = size 440 750, class:^(fmlev-overlay)$
  windowrule = move 40 40, class:^(fmlev-overlay)$

- Sway / i3:
  Add to ~/.config/sway/config or ~/.config/i3/config:
  for_window [class="fmlev-overlay"] floating enable, sticky enable
======================================================================
"#
    }
}
