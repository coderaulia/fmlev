use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Memory error: {0}")]
    Memory(#[from] fmlev_mem::MemoryError),

    #[error("IL2CPP error: {0}")]
    Il2Cpp(#[from] fmlev_il2cpp::Il2CppError),

    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("Player with ID {0} not found")]
    PlayerNotFound(i32),

    #[error("Module '{0}' is not loaded in target process")]
    ModuleNotLoaded(String),

    #[error("No scan candidates remaining")]
    NoScanCandidates,
}

pub type Result<T> = std::result::Result<T, CoreError>;
