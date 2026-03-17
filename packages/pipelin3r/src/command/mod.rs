//! Shell command execution wrapper.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::PipelineError;

/// Environment variable map for command execution.
pub type EnvMap = BTreeMap<String, String>;

/// Configuration for a shell command.
#[derive(Debug, Clone)]
pub struct CommandConfig {
    /// Program to execute.
    pub program: String,
    /// Arguments to pass to the program.
    pub args: Vec<String>,
    /// Working directory for execution.
    pub work_dir: Option<PathBuf>,
    /// Additional environment variables.
    pub env: Option<EnvMap>,
    /// Execution timeout.
    pub timeout: Option<Duration>,
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

impl CommandConfig {
    /// Create a new command configuration for the given program.
    ///
    /// All optional fields default to `None` and args default to empty.
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            work_dir: None,
            env: None,
            timeout: None,
        }
    }
}

/// Execute a shell command.
///
/// Spawns the configured program as a subprocess, optionally setting
/// the working directory, environment variables, and a timeout.
///
/// # Errors
/// Returns an error if the command cannot be spawned or if the timeout expires.
pub async fn run_command(config: &CommandConfig) -> Result<CommandResult, PipelineError> {
    let mut cmd = tokio::process::Command::new(&config.program);
    let _ = cmd.args(&config.args);

    if let Some(dir) = &config.work_dir {
        let _ = cmd.current_dir(dir);
    }

    if let Some(env_vars) = &config.env {
        for (key, value) in env_vars {
            let _ = cmd.env(key, value);
        }
    }

    let output = if let Some(duration) = config.timeout {
        tokio::time::timeout(duration, cmd.output())
            .await
            .map_err(|_| {
                PipelineError::Command(format!(
                    "command '{}' timed out after {duration:?}",
                    config.program
                ))
            })??
    } else {
        cmd.output().await?
    };

    let exit_code = output.status.code();

    Ok(CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code,
    })
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
