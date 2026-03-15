//! Agent builder for single and batch LLM agent invocations.

use std::path::{Path, PathBuf};
use std::time::Duration;

use shedul3r_rs_sdk::TaskPayload;

use crate::auth::{merge_env, Auth};
use crate::bundle::Bundle;
use crate::executor::{extract_step_name, Executor};
use crate::task::{build_task_yaml, TaskConfig};

/// Result of an agent invocation.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// Whether the agent completed successfully.
    pub success: bool,
    /// Agent output text (or error message on failure).
    pub output: String,
}

impl AgentResult {
    /// Return a reference to self if successful, or an error if not.
    ///
    /// # Errors
    /// Returns an error containing the output text if the agent failed.
    pub fn require_success(&self) -> anyhow::Result<&Self> {
        if self.success {
            Ok(self)
        } else {
            anyhow::bail!("agent failed: {}", self.output);
        }
    }
}

/// Builder for configuring and executing a single agent invocation.
#[must_use]
pub struct AgentBuilder<'a> {
    executor: &'a Executor,
    name: String,
    auth: Option<&'a Auth>,
    model: Option<String>,
    timeout: Option<Duration>,
    tools: Option<String>,
    prompt: Option<String>,
    working_dir: Option<PathBuf>,
    expected_output: Option<PathBuf>,
    bundle_data: Option<Bundle>,
}

impl<'a> AgentBuilder<'a> {
    /// Create a new agent builder (called by [`Executor::agent`]).
    pub(crate) fn new(executor: &'a Executor, name: &str) -> Self {
        Self {
            executor,
            name: String::from(name),
            auth: None,
            model: None,
            timeout: None,
            tools: None,
            prompt: None,
            working_dir: None,
            expected_output: None,
            bundle_data: None,
        }
    }

    /// Override the default auth for this invocation.
    pub const fn auth(mut self, auth: &'a Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Set the LLM model (e.g. `"opus"`, `"sonnet"`).
    pub fn model(mut self, model: &str) -> Self {
        self.model = Some(String::from(model));
        self
    }

    /// Set the task timeout.
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the allowed tools (comma-separated list).
    pub fn tools(mut self, tools: &[&str]) -> Self {
        self.tools = Some(tools.join(","));
        self
    }

    /// Set the prompt text to send to the agent.
    pub fn prompt(mut self, text: &str) -> Self {
        self.prompt = Some(String::from(text));
        self
    }

    /// Set the working directory for the agent.
    pub fn working_dir(mut self, path: &Path) -> Self {
        self.working_dir = Some(path.to_path_buf());
        self
    }

    /// Set the expected output file path for file-poll recovery.
    pub fn expected_output(mut self, path: &Path) -> Self {
        self.expected_output = Some(path.to_path_buf());
        self
    }

    /// Attach a bundle of files to the invocation.
    pub fn bundle(mut self, bundle: Bundle) -> Self {
        self.bundle_data = Some(bundle);
        self
    }

    /// Execute the agent invocation.
    ///
    /// 1. Builds task YAML from model/timeout/tools config
    /// 2. Gets auth env vars (from builder override or executor default)
    /// 3. If dry-run: writes capture files to disk
    /// 4. Otherwise: calls SDK's `submit_task_with_recovery`
    ///
    /// # Errors
    /// Returns an error if task YAML building fails or the SDK call fails.
    pub async fn execute(self) -> anyhow::Result<AgentResult> {
        let prompt = self
            .prompt
            .ok_or_else(|| anyhow::anyhow!("agent prompt is required"))?;

        let timeout_str = self.timeout.map(format_duration);

        let task_yaml = build_task_yaml(&TaskConfig {
            name: self.name.clone(),
            model: self.model,
            timeout: timeout_str,
            provider_id: None,
            max_concurrent: None,
            max_wait: None,
            max_retries: None,
            allowed_tools: self.tools,
        })?;

        // Resolve auth: builder override > executor default > empty.
        let auth = self.auth.or_else(|| self.executor.default_auth());
        let auth_env = auth.map_or_else(
            std::collections::HashMap::new,
            Auth::to_env,
        );

        let env = merge_env(auth_env, None);

        // Dry-run: capture to disk.
        if let Some(dry_run_mutex) = self.executor.dry_run_config() {
            let mut guard = dry_run_mutex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let step_name = extract_step_name(&task_yaml);
            let index = guard.counter;
            guard.counter = guard.counter.saturating_add(1);

            let capture_dir = guard.base_dir.join(&step_name).join(index.to_string());
            drop(guard); // Release lock before I/O.

            std::fs::create_dir_all(&capture_dir)?;
            std::fs::write(capture_dir.join("prompt.md"), &prompt)?;
            std::fs::write(capture_dir.join("task.yaml"), &task_yaml)?;

            let meta = serde_json::json!({
                "expectedOutput": self.expected_output.as_ref().map(|p| p.display().to_string()),
                "workingDirectory": self.working_dir.as_ref().map(|p| p.display().to_string()),
            });
            std::fs::write(
                capture_dir.join("meta.json"),
                serde_json::to_string_pretty(&meta)?,
            )?;

            tracing::info!("[dry-run] Captured to {}", capture_dir.display());
            return Ok(AgentResult {
                success: true,
                output: String::from("(dry-run)"),
            });
        }

        // Real execution via SDK.
        let payload = TaskPayload {
            task: task_yaml,
            input: prompt,
            working_directory: self.working_dir.map(|p| p.display().to_string()),
            environment: env,
        };

        let result = if let Some(expected) = &self.expected_output {
            self.executor
                .sdk_client()
                .submit_task_with_recovery(&payload, expected)
                .await?
        } else {
            self.executor.sdk_client().submit_task(&payload).await?
        };

        Ok(AgentResult {
            success: result.success,
            output: result.output,
        })
    }
}

/// Format a `Duration` as a human-readable timeout string for task YAML.
fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs.checked_div(3600).unwrap_or(0);
    let remaining = total_secs.saturating_sub(hours.saturating_mul(3600));
    let minutes = remaining.checked_div(60).unwrap_or(0);
    let seconds = remaining.saturating_sub(minutes.saturating_mul(60));

    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{minutes}m{seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_result_require_success_ok() {
        let result = AgentResult {
            success: true,
            output: String::from("done"),
        };
        assert!(
            result.require_success().is_ok(),
            "should return Ok for successful agent"
        );
    }

    #[test]
    fn agent_result_require_success_err() {
        let result = AgentResult {
            success: false,
            output: String::from("timeout exceeded"),
        };
        let err = result.require_success();
        assert!(err.is_err(), "should return Err for failed agent");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("timeout exceeded"),
            "error should contain output: {msg}"
        );
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(
            format_duration(Duration::from_secs(900)),
            "15m",
            "15 minutes"
        );
    }

    #[test]
    fn format_duration_hours_and_minutes() {
        assert_eq!(
            format_duration(Duration::from_secs(5400)),
            "1h30m",
            "1 hour 30 minutes"
        );
    }

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(
            format_duration(Duration::from_secs(45)),
            "45s",
            "45 seconds"
        );
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s", "zero");
    }

    #[test]
    fn format_duration_exact_hour() {
        assert_eq!(
            format_duration(Duration::from_secs(3600)),
            "1h",
            "exact hour"
        );
    }
}
