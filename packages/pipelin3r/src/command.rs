//! Shell command execution wrapper.

use std::path::{Path, PathBuf};

use crate::error::PipelineError;

/// Builder for executing shell commands.
#[must_use]
pub struct CommandBuilder {
    program: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
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
            working_dir: None,
        }
    }

    /// Add arguments to the command.
    pub fn args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| String::from(*s)));
        self
    }

    /// Set the working directory for command execution.
    pub fn working_dir(mut self, path: &Path) -> Self {
        self.working_dir = Some(path.to_path_buf());
        self
    }

    /// Execute the command and return the result.
    ///
    /// # Errors
    /// Returns an error if the command cannot be spawned.
    pub async fn execute(self) -> Result<CommandResult, PipelineError> {
        let mut cmd = tokio::process::Command::new(&self.program);
        let _ = cmd.args(&self.args);

        if let Some(dir) = &self.working_dir {
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_command_succeeds() {
        let result = CommandBuilder::new("echo")
            .args(&["hello", "world"])
            .execute()
            .await;

        assert!(result.is_ok(), "echo should not fail to spawn");
        let cmd_result = result.unwrap_or_else(|_| CommandResult {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        });
        assert!(cmd_result.success, "echo should succeed");
        assert_eq!(
            cmd_result.stdout.trim(),
            "hello world",
            "echo output should match"
        );
        assert_eq!(cmd_result.exit_code, Some(0), "exit code should be 0");
    }

    #[tokio::test]
    async fn false_command_fails() {
        let result = CommandBuilder::new("false").execute().await;

        assert!(result.is_ok(), "false should not fail to spawn");
        let cmd_result = result.unwrap_or_else(|_| CommandResult {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        });
        assert!(!cmd_result.success, "false should report failure");
    }

    #[test]
    fn require_success_on_success() {
        let result = CommandResult {
            success: true,
            stdout: String::from("ok"),
            stderr: String::new(),
            exit_code: Some(0),
        };
        assert!(
            result.require_success().is_ok(),
            "should return Ok for successful command"
        );
    }

    #[test]
    fn require_success_on_failure() {
        let result = CommandResult {
            success: false,
            stdout: String::new(),
            stderr: String::from("something went wrong"),
            exit_code: Some(1),
        };
        let err = result.require_success();
        assert!(err.is_err(), "should return Err for failed command");
    }
}
