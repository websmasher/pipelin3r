//! Request and response types for the task execution API.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::EnvironmentMap;
use crate::duration_serde;

/// Inbound request to execute a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    /// YAML task definition string or file path.
    pub task: String,
    /// Optional JSON string to pass as stdin to the subprocess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// Override the limiter key from the task definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limiter_key: Option<String>,
    /// Additional environment variables for the subprocess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentMap>,
    /// Working directory for the subprocess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    /// Timeout override in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Response returned after executing a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    /// Whether the task completed successfully (exit code 0).
    pub success: bool,
    /// Combined stdout output from the subprocess.
    pub output: String,
    /// Execution timing and exit information.
    pub metadata: ExecutionMetadata,
}

/// Metadata about a completed task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// ISO-8601 timestamp when execution started.
    pub started_at: String,
    /// Wall-clock time elapsed during execution.
    #[serde(with = "duration_serde")]
    pub elapsed: Duration,
    /// Process exit code (0 = success).
    pub exit_code: i32,
}

/// Current status of the scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatus {
    /// Number of tasks currently executing.
    pub active_tasks: u32,
    /// Number of tasks waiting in queue.
    pub pending_tasks: u32,
    /// ISO-8601 timestamp when the scheduler started.
    pub started_at: String,
}

/// Status of a specific limiter key's resilience components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimiterKeyStatus {
    /// The limiter key being reported on.
    pub key: String,
    /// Number of rate limit permits currently available.
    pub available_permissions: u32,
    /// Whether the circuit breaker is in the open (tripped) state.
    pub circuit_open: bool,
}
