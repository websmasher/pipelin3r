//! Centralized filesystem operations for the REST adapter.
//!
//! All `std::fs` usage is routed through this module to ensure consistent
//! error handling, testability, and auditability.

use std::path::Path;

/// Read the entire contents of a file as a UTF-8 string.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the file cannot be read.
#[allow(clippy::disallowed_methods)] // centralized fs module: this IS the approved call site
pub fn read_to_string(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
