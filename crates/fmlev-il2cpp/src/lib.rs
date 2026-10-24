pub mod error;
pub mod metadata;
pub mod registry;
pub mod types;

pub use error::{Il2CppError, Result};
pub use registry::{ClassOffsets, OffsetField, OffsetsProfile, OffsetsRegistry, SingletonOffset};
pub use types::{
    read_cstring, read_static_singleton, read_static_singleton_instance, resolve_rva,
    resolve_singleton, Il2CppArray, Il2CppClass, Il2CppObject, Il2CppString,
};
