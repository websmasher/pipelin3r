//! Centralized filesystem operations.
//!
//! All `std::fs` usage is routed through this module to ensure consistent
//! error handling, testability, and auditability. Direct `std::fs::*` calls
//! elsewhere in the crate are banned by clippy and guardrail3.

use std::path::Path;

/// Read the entire contents of a file as a UTF-8 string.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the file cannot be read.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn read_to_string(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

/// Read the entire contents of a file as raw bytes.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the file cannot be read.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
#[allow(clippy::type_complexity, reason = "return type mirrors std::fs::read")]
pub fn read(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    std::fs::read(path)
}

/// Write data to a file, creating it if it does not exist.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the file cannot be written.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), std::io::Error> {
    std::fs::write(path, contents)
}

/// Create a directory and all parent directories.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory cannot be created.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn create_dir_all(path: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path)
}

/// Canonicalize a path, resolving symlinks and `..` components.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the path cannot be resolved.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn canonicalize(path: &Path) -> Result<std::path::PathBuf, std::io::Error> {
    std::fs::canonicalize(path)
}

/// Read the entries of a directory.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory cannot be read.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn read_dir(path: &Path) -> Result<std::fs::ReadDir, std::io::Error> {
    std::fs::read_dir(path)
}

/// Recursively remove a directory and all its contents.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory cannot be removed.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn remove_dir_all(path: &Path) -> Result<(), std::io::Error> {
    std::fs::remove_dir_all(path)
}

/// Copy a file from `src` to `dst`, returning the number of bytes copied.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the file cannot be copied.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn copy(src: &Path, dst: &Path) -> Result<u64, std::io::Error> {
    std::fs::copy(src, dst)
}

/// Read metadata for a path.
///
/// # Errors
///
/// Returns [`std::io::Error`] if metadata cannot be read.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn metadata(path: &Path) -> Result<std::fs::Metadata, std::io::Error> {
    std::fs::metadata(path)
}
