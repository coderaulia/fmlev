use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Football Manager process not found")]
    ProcessNotFound,

    #[error("Permission denied accessing process {0} memory. Check kernel.yama.ptrace_scope or setcap")]
    PermissionDenied(i32),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Module '{0}' not found in /proc/{1}/maps")]
    ModuleNotFound(String, i32),

    #[error("Failed to read {0} bytes at address 0x{1:x}: {2}")]
    ReadFailed(usize, usize, String),

    #[error("Failed to write {0} bytes at address 0x{1:x}: {2}")]
    WriteFailed(usize, usize, String),

    #[error("Invalid pointer dereference at 0x{0:x}")]
    InvalidPointer(usize),

    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
