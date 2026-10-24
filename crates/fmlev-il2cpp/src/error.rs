use thiserror::Error;

#[derive(Error, Debug)]
pub enum Il2CppError {
    #[error("Memory error: {0}")]
    Memory(#[from] fmlev_mem::MemoryError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid metadata magic header: expected 0xFAB11BAF, found 0x{0:x}")]
    InvalidMagic(u32),

    #[error("Unsupported metadata version: {0}")]
    UnsupportedVersion(u32),

    #[error("Offset definition not found for class '{0}' or field '{1}'")]
    OffsetNotFound(String, String),

    #[error("Singleton definition '{0}' not found")]
    SingletonNotFound(String),

    #[error("Encountered null pointer at 0x{0:x}")]
    NullPointer(usize),
}

pub type Result<T> = std::result::Result<T, Il2CppError>;
