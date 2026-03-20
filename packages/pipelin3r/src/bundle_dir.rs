//! RAII guard for ephemeral bundle work directories.
//!
//! Creates a `.bundle-{slug}` directory under a given parent and removes it on
//! drop, ensuring cleanup even on early return or panic.

use std::path::{Path, PathBuf};

use crate::error::PipelineError;

/// RAII guard for an ephemeral work directory.
///
/// Creates a `.bundle-{slug}` directory under the given parent on construction.
/// The directory is recursively removed when the guard is dropped, even if the
/// owning scope exits via panic or early return.
///
/// # Examples
///
/// ```no_run
/// # use std::path::Path;
/// # use pipelin3r::bundle_dir::BundleDir;
/// let bundle = BundleDir::new(Path::new("/tmp"), "my-task").unwrap();
/// // use bundle.path() to read/write files
/// // directory is removed when `bundle` goes out of scope
/// ```
#[derive(Debug)]
pub struct BundleDir {
    /// Absolute path to the created directory.
    path: PathBuf,
}

impl BundleDir {
    /// Create a new bundle directory under `parent` named `.bundle-{slug}`.
    ///
    /// The parent directory must already exist. The slug is sanitised to
    /// replace path-separator characters, but callers should prefer
    /// alphanumeric-plus-hyphen slugs.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Transport`] if the directory cannot be created.
    pub fn new(parent: &Path, slug: &str) -> Result<Self, PipelineError> {
        let dir_name = format!(".bundle-{slug}");
        let path = parent.join(dir_name);
        crate::fs::create_dir_all(&path).map_err(|e| {
            PipelineError::Transport(format!(
                "failed to create bundle dir {}: {e}",
                path.display()
            ))
        })?;
        Ok(Self { path })
    }

    /// Get the path to the bundle directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BundleDir {
    fn drop(&mut self) {
        let _ = crate::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[path = "bundle_dir_tests.rs"]
mod tests;
