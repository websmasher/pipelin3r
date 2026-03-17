//! Shell command execution wrapper.

use std::path::{Path, PathBuf};

use crate::error::PipelineError;

/// Builder for executing shell commands.
#[must_use]
pub struct CommandBuilder {
    program: String,
    args: Vec<String>,
    work_dir: Option<PathBuf>,
}

/// Result of a shell command execution.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Whether the command exited successfully (exit code 0).
    pub success: bool,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Exit code, if available.
    pub exit_code: Option<i32>,
}

impl CommandBuilder {
    /// Create a new command builder for the given program.
    pub fn new(program: &str) -> Self {
        Self {
            program: String::from(program),
            args: Vec::new(),
            work_dir: None,
        }
    }

    /// Add arguments to the command.
    pub fn args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| String::from(*s)));
        self
    }

    /// Set the work directory for command execution.
    pub fn work_dir(mut self, path: &Path) -> Self {
        self.work_dir = Some(path.to_path_buf());
        self
    }

    /// Execute the command and return the result.
    ///
    /// # Errors
    /// Returns an error if the command cannot be spawned.
    pub async fn execute(self) -> Result<CommandResult, PipelineError> {
        let mut cmd = tokio::process::Command::new(&self.program);
        let _ = cmd.args(&self.args);

        if let Some(dir) = &self.work_dir {
            let _ = cmd.current_dir(dir);
        }

        let output = cmd.output().await?;

        let exit_code = output.status.code();

        Ok(CommandResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code,
        })
    }
}

impl CommandResult {
    /// Return a reference to self if successful, or an error if not.
    ///
    /// # Errors
    /// Returns an error containing stderr if the command failed.
    pub fn require_success(&self) -> Result<&Self, PipelineError> {
        if self.success {
            Ok(self)
        } else {
            let code_str = self
                .exit_code
                .map_or_else(|| String::from("unknown"), |c| c.to_string());
            Err(PipelineError::Command(format!(
                "failed (exit code {code_str}): {}",
                self.stderr.trim()
            )))
        }
    }
}

#[cfg(test)]
mod tests;
