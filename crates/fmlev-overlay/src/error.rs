use thiserror::Error;

#[derive(Error, Debug)]
pub enum OverlayError {
    #[error("Failed to launch overlay browser process: {0}")]
    LaunchFailed(String),

    #[error("No supported browser found on system for app-mode overlay")]
    BrowserNotFound,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, OverlayError>;
