//! Bundle builder for packaging files to send alongside agent invocations.
//!
//! Bundles are currently written to a local temporary directory. Remote upload
//! (via SDK bundle endpoints) will be added when the server supports it.

use std::path::PathBuf;

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
    pub fn add_file(mut self, path: &str, content: &[u8]) -> Self {
        self.files.push((String::from(path), content.to_vec()));
        self
    }

    /// Add a text file to the bundle.
    pub fn add_text_file(mut self, path: &str, content: &str) -> Self {
        self.files
            .push((String::from(path), content.as_bytes().to_vec()));
        self
    }

    /// Register an expected output path (relative to the working directory).
    pub fn expected_output(mut self, path: &str) -> Self {
        self.expected_outputs.push(String::from(path));
        self
    }

    /// Write bundle files to a local temporary directory.
    ///
    /// Returns the path to the temporary directory.
    ///
    /// # Errors
    /// Returns an error if directory creation or file writing fails.
    pub fn write_to_temp_dir(&self) -> anyhow::Result<PathBuf> {
        let temp_dir = std::env::temp_dir().join(format!(
            "pipelin3r-bundle-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_dir)?;

        for (path, content) in &self.files {
            let file_path = temp_dir.join(path);
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
    fn add_files_and_outputs() {
        let bundle = Bundle::new()
            .add_file("data.bin", &[0x00, 0x01, 0x02])
            .add_text_file("prompt.md", "Hello world")
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
    fn write_to_temp_dir_creates_files() {
        let bundle = Bundle::new()
            .add_text_file("hello.txt", "world")
            .add_text_file("sub/nested.txt", "deep");

        let result = bundle.write_to_temp_dir();
        assert!(result.is_ok(), "should succeed writing to temp dir");

        let dir = result.unwrap_or_else(|_| PathBuf::new());
        assert!(dir.join("hello.txt").exists(), "hello.txt should exist");
        assert!(
            dir.join("sub/nested.txt").exists(),
            "nested file should exist"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
