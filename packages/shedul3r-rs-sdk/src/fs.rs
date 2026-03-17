//! Centralized filesystem operations.
//!
//! All `std::fs` usage is routed through this module to ensure consistent
//! error handling, testability, and auditability.

use std::path::Path;

/// Remove a file, ignoring errors (best-effort cleanup).
///
/// Returns `Ok(())` on success or the underlying I/O error on failure.
#[allow(
    clippy::disallowed_methods,
    reason = "centralized fs module: this IS the approved call site"
)]
pub fn remove_file(path: &Path) -> Result<(), std::io::Error> {
    std::fs::remove_file(path)
}
