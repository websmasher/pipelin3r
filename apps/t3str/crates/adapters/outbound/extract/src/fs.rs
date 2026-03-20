//! Thin filesystem wrappers for extract adapter operations.

use std::path::Path;
use t3str_domain_types::T3strError;

/// Read directory entries, returning an iterator.
pub fn read_dir(path: &Path) -> Result<std::fs::ReadDir, T3strError> {
    std::fs::read_dir(path).map_err(T3strError::Io)
}

/// Read a file's entire contents as a string.
pub fn read_to_string(path: &Path) -> Result<String, T3strError> {
    std::fs::read_to_string(path).map_err(T3strError::Io)
}
