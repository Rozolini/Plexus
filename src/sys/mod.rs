#[cfg(windows)]
mod windows_mmap;

#[cfg(windows)]
pub use windows_mmap::{
    MapAccess, MappingError, MappingMetadata, MappingSecurityProfile, SharedMapping,
};
