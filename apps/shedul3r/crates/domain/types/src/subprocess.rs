//! Types for subprocess execution.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::EnvironmentMap;
use crate::duration_serde;

/// A fully resolved command ready for subprocess execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessCommand {
    /// The command and its arguments (e.g. `["/bin/sh", "-c", "echo hello"]`).
    pub command: Vec<String>,
    /// Optional working directory for the child process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    /// Optional environment variables to set in the child process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentMap>,
    /// Optional timeout after which the process is killed.
    #[serde(
        default,
        with = "duration_serde::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout: Option<Duration>,
    /// Optional data to write to the child process's stdin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_data: Option<String>,
}

/// Result of a completed subprocess execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessResult {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Wall-clock time elapsed during execution.
    #[serde(with = "duration_serde")]
    pub elapsed: Duration,
}
