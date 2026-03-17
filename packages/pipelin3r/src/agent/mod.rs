//! Agent configuration and execution for LLM agent invocations.

mod execute;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::auth::{Auth, EnvironmentMap};
use crate::error::PipelineError;
use crate::model::Model;

pub(crate) use execute::{
    execute_dry_run_capture, execute_with_work_dir, format_duration, validate_work_dir,
};

/// Configuration for a single agent invocation.
///
/// Required fields (`name`, `prompt`) are set via the constructor.
/// Optional fields use struct update syntax from a defaults instance.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    // ── Required (set via constructor) ──
    /// Step name for logging and dry-run capture.
    pub name: String,
    /// Prompt text sent as stdin to the agent subprocess.
    pub prompt: String,

    // ── Agent settings ──
    /// LLM model to use.
    pub model: Option<Model>,
    /// Work directory path (the workspace channel).
    pub work_dir: Option<PathBuf>,
    /// Agent execution timeout (goes into task YAML `timeout:` field).
    pub execution_timeout: Option<Duration>,
    /// Allowed tools for the agent (e.g., `["Read", "Write", "Bash"]`).
    /// `None` = all tools allowed.
    pub tools: Option<Vec<String>>,
    /// Auth override (falls back to executor default).
    pub auth: Option<Auth>,
    /// Additional environment variables for the subprocess.
    pub env: Option<EnvironmentMap>,

    // ── Scheduling settings (go into task YAML for shedul3r) ──
    /// Provider/limiter key for rate limiting grouping (e.g., `"claude"`).
    pub provider_id: Option<String>,
    /// Maximum concurrent tasks for this provider key.
    pub max_concurrent: Option<usize>,
    /// Maximum time to wait in shedul3r's queue.
    pub max_wait: Option<Duration>,
    /// Retry configuration for failed executions.
    pub retry: Option<RetryConfig>,

    // ── Output settings ──
    /// Expected output files (relative to `work_dir`).
    /// After execution: verified to exist, downloaded if remote.
    /// Contents returned in [`AgentResult::output_files`].
    pub expect_outputs: Vec<String>,

    // ── HTTP transport ──
    /// HTTP request timeout to shedul3r (must be > `execution_timeout` + `max_wait`).
    /// Default: 45 minutes.
    pub request_timeout: Option<Duration>,
}

/// Retry configuration for agent tasks.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: usize,
    /// Delay before the first retry.
    pub initial_delay: Duration,
    /// Multiplier applied to the delay after each retry.
    pub backoff_multiplier: f64,
    /// Maximum delay between retries.
    pub max_delay: Duration,
}

impl AgentConfig {
    /// Create a new config with required fields.
    /// All optional fields are `None`/empty.
    pub fn new(name: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            prompt: prompt.into(),
            model: None,
            work_dir: None,
            execution_timeout: None,
            tools: None,
            auth: None,
            env: None,
            provider_id: None,
            max_concurrent: None,
            max_wait: None,
            retry: None,
            expect_outputs: Vec::new(),
            request_timeout: None,
        }
    }
}

/// Result of an agent invocation.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// Whether the agent completed successfully.
    pub success: bool,
    /// Agent output text (or error message on failure).
    pub output: String,
    /// Contents of expected output files (filename -> content).
    /// Populated only for files listed in `expect_outputs` that exist after execution.
    pub output_files: BTreeMap<String, String>,
}

impl AgentResult {
    /// Return a reference to self if successful, or an error if not.
    ///
    /// # Errors
    /// Returns an error containing the output text if the agent failed.
    pub fn require_success(&self) -> Result<&Self, PipelineError> {
        if self.success {
            Ok(self)
        } else {
            Err(PipelineError::AgentFailed {
                message: self.output.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests;
