//! Bundle builder for packaging files to send alongside agent invocations.
//!
//! Bundles are currently written to a local temporary directory. Remote upload
//! (via SDK bundle endpoints) will be added when the server supports it.

use crate::error::PipelineError;

/// Validate that a bundle path contains only normal components (no `..`, `/`, etc.).
///
/// Rejects any path that contains parent traversal, root components, or prefix
/// components. Only relative paths with normal segments are allowed.
///
/// # Errors
/// Returns an error if the path contains non-normal components.
fn validate_path(name: &str) -> Result<(), PipelineError> {
    if name.is_empty() {
        return Err(PipelineError::Bundle(String::from(
            "invalid bundle path: empty",
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
                return Err(PipelineError::Bundle(format!(
                    "invalid bundle path: {name}"
                )))
            }
        }
    }
    Ok(())
}

/// A collection of files to send alongside an agent invocation.
#[derive(Debug, Clone)]
#[must_use]
pub struct Bundle {
    files: Vec<(String, Vec<u8>)>,
    expected_outputs: Vec<String>,
}

impl Bundle {
    /// Create a new empty bundle.
    pub const fn new() -> Self {
        Self {
            files: Vec::new(),
            expected_outputs: Vec::new(),
        }
    }

    /// Add a binary file to the bundle.
    ///
    /// # Errors
    /// Returns an error if the path contains traversal components (e.g. `..`).
    pub fn add_file(mut self, path: &str, content: &[u8]) -> Result<Self, PipelineError> {
        validate_path(path)?;
        self.files.push((String::from(path), content.to_vec()));
        Ok(self)
    }

    /// Add a text file to the bundle.
    ///
    /// # Errors
    /// Returns an error if the path contains traversal components (e.g. `..`).
    pub fn add_text_file(mut self, path: &str, content: &str) -> Result<Self, PipelineError> {
        validate_path(path)?;
        self.files
            .push((String::from(path), content.as_bytes().to_vec()));
        Ok(self)
    }

    /// Register an expected output path (relative to the working directory).
    pub fn expected_output(mut self, path: &str) -> Self {
        self.expected_outputs.push(String::from(path));
        self
    }

    /// Write bundle files to a unique temporary directory.
    ///
    /// Returns a [`tempfile::TempDir`] handle. The directory is automatically
    /// deleted when the handle is dropped, so the caller must keep it alive
    /// for as long as the files are needed.
    ///
    /// # Errors
    /// Returns an error if directory creation or file writing fails.
    pub fn write_to_temp_dir(&self) -> Result<tempfile::TempDir, PipelineError> {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            PipelineError::Bundle(format!("failed to create temp dir: {e}"))
        })?;

        for (path, content) in &self.files {
            let file_path = temp_dir.path().join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, content)?;
        }

        Ok(temp_dir)
    }

    /// Get the list of expected output paths.
    pub fn expected_output_paths(&self) -> &[String] {
        &self.expected_outputs
    }

    /// Get the number of files in the bundle.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get a reference to the files in this bundle.
    ///
    /// Each entry is a `(relative_path, content)` pair.
    pub fn files(&self) -> &[(String, Vec<u8>)] {
        &self.files
    }
}

impl Default for Bundle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bundle() {
        let bundle = Bundle::new();
        assert_eq!(bundle.file_count(), 0, "new bundle should be empty");
        assert!(
            bundle.expected_output_paths().is_empty(),
            "no expected outputs"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion on known-Ok value
    fn add_files_and_outputs() {
        let bundle = Bundle::new()
            .add_file("data.bin", &[0x00, 0x01, 0x02])
            .unwrap()
            .add_text_file("prompt.md", "Hello world")
            .unwrap()
            .expected_output("result.json");

        assert_eq!(bundle.file_count(), 2, "should have two files");
        assert_eq!(
            bundle.expected_output_paths().len(),
            1,
            "should have one expected output"
        );
        assert_eq!(
            bundle.expected_output_paths().first().map(String::as_str),
            Some("result.json"),
            "expected output path"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion on known-Ok value
    fn write_to_temp_dir_creates_files() {
        let bundle = Bundle::new()
            .add_text_file("hello.txt", "world")
            .unwrap()
            .add_text_file("sub/nested.txt", "deep")
            .unwrap();

        let result = bundle.write_to_temp_dir();
        assert!(result.is_ok(), "should succeed writing to temp dir");

        let dir = result.unwrap_or_else(|_| {
            // Return a dummy TempDir that won't match assertions.
            tempfile::tempdir().unwrap_or_else(|_| std::process::abort())
        });
        assert!(
            dir.path().join("hello.txt").exists(),
            "hello.txt should exist"
        );
        assert!(
            dir.path().join("sub/nested.txt").exists(),
            "nested file should exist"
        );
        // TempDir cleanup is automatic on drop.
    }

    #[test]
    fn path_traversal_parent_rejected() {
        let result = Bundle::new().add_file("../../etc/shadow", b"evil");
        assert!(result.is_err(), "parent traversal should be rejected");
        let msg = result.unwrap_or_else(|e| {
            assert!(
                e.to_string().contains("invalid bundle path"),
                "error should mention invalid path: {e}"
            );
            Bundle::new()
        });
        assert_eq!(msg.file_count(), 0, "no files should be added");
    }

    #[test]
    fn path_traversal_absolute_rejected() {
        let result = Bundle::new().add_text_file("/etc/passwd", "evil");
        assert!(result.is_err(), "absolute path should be rejected");
    }

    #[test]
    fn path_traversal_dot_rejected() {
        let result = Bundle::new().add_file("./something", b"data");
        assert!(result.is_err(), "current-dir component should be rejected");
    }

    #[test]
    fn path_empty_rejected() {
        let result = Bundle::new().add_file("", b"data");
        assert!(result.is_err(), "empty path should be rejected");
    }

    #[test]
    fn validate_path_normal_nested() {
        assert!(
            validate_path("sub/dir/file.txt").is_ok(),
            "normal nested path should be valid"
        );
    }
}
