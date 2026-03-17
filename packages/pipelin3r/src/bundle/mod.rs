//! Path validation utilities for file transport.
//!
//! Internal module providing path safety checks used by the work-dir
//! transport logic in [`crate::agent::execute`].

use crate::error::PipelineError;

/// Validate that a relative path contains only normal components (no `..`, `/`, etc.).
///
/// Rejects any path that contains parent traversal, root components, or prefix
/// components. Only relative paths with normal segments are allowed.
///
/// # Errors
/// Returns an error if the path is empty or contains non-normal components.
pub fn validate_path(name: &str) -> Result<(), PipelineError> {
    if name.is_empty() {
        return Err(PipelineError::Transport(String::from(
            "invalid path: empty",
        )));
    }
    let path = std::path::Path::new(name);
    for component in path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::CurDir
            | std::path::Component::ParentDir => {
                return Err(PipelineError::Transport(format!("invalid path: {name}")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
