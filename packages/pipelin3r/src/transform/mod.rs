//! Pure function transforms for deterministic data processing.
//!
//! A transform reads input files, applies a pure Rust function, and writes
//! output files. No LLM, no subprocess — used for filtering, merging,
//! deduplication, and coverage analysis.

use std::path::{Path, PathBuf};

use crate::error::PipelineError;

/// Input/output pair: a file path and its byte content.
type FilePair = (PathBuf, Vec<u8>);

/// Boxed transform function type.
type TransformFn = Box<dyn FnOnce(Vec<FilePair>) -> Result<Vec<FilePair>, PipelineError> + Send>;

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
        F: FnOnce(Vec<FilePair>) -> Result<Vec<FilePair>, PipelineError> + Send + 'static,
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
    pub fn execute(self) -> Result<TransformResult, PipelineError> {
        let transform_fn = self.transform_fn.ok_or_else(|| {
            PipelineError::Transform(format!("'{}': no apply function set", self.name))
        })?;

        // Read all input files.
        let mut inputs = Vec::with_capacity(self.input_files.len());
        for path in &self.input_files {
            let content = crate::fs::read(path).map_err(|e| {
                PipelineError::Transform(format!(
                    "'{}': failed to read {}: {e}",
                    self.name,
                    path.display()
                ))
            })?;
            inputs.push((path.clone(), content));
        }
        let files_read = inputs.len();

        tracing::info!("[transform] '{}': read {files_read} input files", self.name);

        // Apply the transform.
        let outputs = transform_fn(inputs)?;
        let files_written = outputs.len();

        // Write all output files.
        for (path, content) in &outputs {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    crate::fs::create_dir_all(parent).map_err(|e| {
                        PipelineError::Transform(format!(
                            "'{}': failed to create directory {}: {e}",
                            self.name,
                            parent.display()
                        ))
                    })?;
                }
            }
            crate::fs::write(path, content).map_err(|e| {
                PipelineError::Transform(format!(
                    "'{}': failed to write {}: {e}",
                    self.name,
                    path.display()
                ))
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
mod tests;
