//! Pure function transforms for deterministic data processing.
//!
//! A transform reads input files, applies a pure Rust function, and writes
//! output files. No LLM, no subprocess — used for filtering, merging,
//! deduplication, and coverage analysis.

use std::path::{Path, PathBuf};

/// Input/output pair: a file path and its byte content.
type FilePair = (PathBuf, Vec<u8>);

/// Boxed transform function type.
type TransformFn = Box<dyn FnOnce(Vec<FilePair>) -> anyhow::Result<Vec<FilePair>> + Send>;

/// Builder for a pure-function transform step.
///
/// Reads input files, applies a transform function, and writes output files.
#[must_use]
pub struct TransformBuilder {
    name: String,
    input_files: Vec<PathBuf>,
    transform_fn: Option<TransformFn>,
}

/// Result of a transform execution.
#[derive(Debug, Clone)]
pub struct TransformResult {
    /// Number of input files read.
    pub files_read: usize,
    /// Number of output files written.
    pub files_written: usize,
}

impl TransformBuilder {
    /// Create a new transform builder with the given step name.
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            input_files: Vec::new(),
            transform_fn: None,
        }
    }

    /// Add a single input file to read.
    pub fn input_file(mut self, path: &Path) -> Self {
        self.input_files.push(path.to_path_buf());
        self
    }

    /// Add multiple input files to read.
    pub fn input_files(mut self, paths: &[PathBuf]) -> Self {
        self.input_files.extend_from_slice(paths);
        self
    }

    /// Set the transform function.
    ///
    /// Receives `(path, content)` pairs for each input file and returns
    /// `(output_path, content)` pairs to write to disk.
    pub fn apply<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Vec<FilePair>) -> anyhow::Result<Vec<FilePair>> + Send + 'static,
    {
        self.transform_fn = Some(Box::new(f));
        self
    }

    /// Execute the transform: read inputs, apply function, write outputs.
    ///
    /// 1. Reads all input files into `(PathBuf, Vec<u8>)` pairs
    /// 2. Calls the transform function
    /// 3. Writes all output `(PathBuf, Vec<u8>)` pairs to disk
    /// 4. Returns counts of files read and written
    ///
    /// # Errors
    /// Returns an error if:
    /// - No transform function was set via [`apply`](Self::apply)
    /// - Any input file cannot be read
    /// - The transform function returns an error
    /// - Any output file cannot be written
    pub fn execute(self) -> anyhow::Result<TransformResult> {
        let transform_fn = self
            .transform_fn
            .ok_or_else(|| anyhow::anyhow!("transform '{}': no apply function set", self.name))?;

        // Read all input files.
        let mut inputs = Vec::with_capacity(self.input_files.len());
        for path in &self.input_files {
            let content = std::fs::read(path)
                .map_err(|e| anyhow::anyhow!("transform '{}': failed to read {}: {e}", self.name, path.display()))?;
            inputs.push((path.clone(), content));
        }
        let files_read = inputs.len();

        tracing::info!(
            "[transform] '{}': read {files_read} input files",
            self.name
        );

        // Apply the transform.
        let outputs = transform_fn(inputs)?;
        let files_written = outputs.len();

        // Write all output files.
        for (path, content) in &outputs {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        anyhow::anyhow!(
                            "transform '{}': failed to create directory {}: {e}",
                            self.name,
                            parent.display()
                        )
                    })?;
                }
            }
            std::fs::write(path, content).map_err(|e| {
                anyhow::anyhow!(
                    "transform '{}': failed to write {}: {e}",
                    self.name,
                    path.display()
                )
            })?;
        }

        tracing::info!(
            "[transform] '{}': wrote {files_written} output files",
            self.name
        );

        Ok(TransformResult {
            files_read,
            files_written,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion with tempdir
    fn filter_files_reduces_count() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // Write 3 input files.
        let input_a = base.join("a.txt");
        let input_b = base.join("b.txt");
        let input_c = base.join("c.txt");
        std::fs::write(&input_a, b"keep-a").unwrap();
        std::fs::write(&input_b, b"skip-b").unwrap();
        std::fs::write(&input_c, b"keep-c").unwrap();

        let out_dir = base.join("out");

        let result = TransformBuilder::new("filter-test")
            .input_file(&input_a)
            .input_file(&input_b)
            .input_file(&input_c)
            .apply(move |inputs| {
                let outputs: Vec<(PathBuf, Vec<u8>)> = inputs
                    .into_iter()
                    .filter(|(_, content)| content.starts_with(b"keep"))
                    .map(|(path, content)| {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        (out_dir.join(name), content)
                    })
                    .collect();
                Ok(outputs)
            })
            .execute()
            .unwrap();

        assert_eq!(result.files_read, 3, "should read all 3 input files");
        assert_eq!(result.files_written, 2, "should write only 2 filtered files");

        // Verify the right files were written.
        assert!(
            base.join("out").join("a.txt").exists(),
            "a.txt should exist in output"
        );
        assert!(
            !base.join("out").join("b.txt").exists(),
            "b.txt should be filtered out"
        );
        assert!(
            base.join("out").join("c.txt").exists(),
            "c.txt should exist in output"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion with tempdir
    fn modify_content_uppercase() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let input = base.join("hello.txt");
        std::fs::write(&input, b"hello world").unwrap();

        let output_path = base.join("out").join("hello.txt");
        let output_path_clone = output_path.clone();

        let result = TransformBuilder::new("uppercase-test")
            .input_file(&input)
            .apply(move |inputs| {
                let outputs: Vec<(PathBuf, Vec<u8>)> = inputs
                    .into_iter()
                    .map(|(_, content)| {
                        let upper: Vec<u8> = content
                            .iter()
                            .map(u8::to_ascii_uppercase)
                            .collect();
                        (output_path_clone.clone(), upper)
                    })
                    .collect();
                Ok(outputs)
            })
            .execute()
            .unwrap();

        assert_eq!(result.files_read, 1, "should read 1 file");
        assert_eq!(result.files_written, 1, "should write 1 file");

        let written = std::fs::read(&output_path).unwrap();
        assert_eq!(
            written,
            b"HELLO WORLD",
            "content should be uppercased"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion
    fn empty_inputs_returns_empty() {
        let result = TransformBuilder::new("empty-test")
            .apply(|inputs| {
                assert!(inputs.is_empty(), "should receive no inputs");
                Ok(Vec::new())
            })
            .execute()
            .unwrap();

        assert_eq!(result.files_read, 0, "should read 0 files");
        assert_eq!(result.files_written, 0, "should write 0 files");
    }

    #[test]
    fn missing_apply_returns_error() {
        let result = TransformBuilder::new("no-apply").execute();
        assert!(result.is_err(), "should fail without apply function");
        let msg = result.unwrap_or_else(|e| {
            // Verify error message, then return a dummy.
            assert!(
                e.to_string().contains("no apply function"),
                "error should mention missing apply: {e}"
            );
            TransformResult {
                files_read: 0,
                files_written: 0,
            }
        });
        assert_eq!(msg.files_read, 0, "dummy result");
    }

    #[test]
    fn input_files_bulk_add() {
        let builder = TransformBuilder::new("bulk")
            .input_files(&[PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]);

        assert_eq!(
            builder.input_files.len(),
            3,
            "should add all files via input_files"
        );
    }
}
