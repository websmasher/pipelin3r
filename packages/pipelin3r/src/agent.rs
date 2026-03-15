//! Agent builder for single and batch LLM agent invocations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use shedul3r_rs_sdk::TaskPayload;

use crate::auth::{merge_env, Auth};
use crate::bundle::Bundle;
use crate::error::PipelineError;
use crate::executor::{extract_step_name, Executor};
use crate::model::Model;
use crate::pool::run_pool;
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
    pub fn require_success(&self) -> Result<&Self, PipelineError> {
        if self.success {
            Ok(self)
        } else {
            Err(PipelineError::Other(format!("agent failed: {}", self.output)))
        }
    }
}

/// Per-item task configuration for batch agent invocations.
///
/// Carries the prompt and optional overrides for a single item in a batch.
/// Built via chained setter methods.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct AgentTask {
    /// Prompt text for this task.
    pub(crate) prompt: Option<String>,
    /// Working directory override for this task.
    pub(crate) working_dir: Option<PathBuf>,
    /// Expected output file path for file-poll recovery.
    pub(crate) expected_output: Option<PathBuf>,
    /// Bundle of files to attach.
    pub(crate) bundle_data: Option<Bundle>,
    /// Auth override for this specific task.
    pub(crate) auth: Option<Auth>,
}

impl AgentTask {
    /// Create a new empty agent task.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the prompt text for this task.
    pub fn prompt(mut self, text: &str) -> Self {
        self.prompt = Some(String::from(text));
        self
    }

    /// Set the working directory for this task.
    pub fn working_dir(mut self, path: &Path) -> Self {
        self.working_dir = Some(path.to_path_buf());
        self
    }

    /// Set the expected output file path for file-poll recovery.
    pub fn expected_output(mut self, path: &Path) -> Self {
        self.expected_output = Some(path.to_path_buf());
        self
    }

    /// Attach a bundle of files to this task.
    pub fn bundle(mut self, bundle: Bundle) -> Self {
        self.bundle_data = Some(bundle);
        self
    }

    /// Override authentication for this specific task.
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }
}

/// Builder for configuring and executing a single agent invocation.
#[must_use]
pub struct AgentBuilder<'a> {
    executor: &'a Executor,
    name: String,
    auth: Option<&'a Auth>,
    model: Option<Model>,
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

    /// Set the LLM model.
    pub fn model(mut self, model: Model) -> Self {
        self.model = Some(model);
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

    /// Switch to batch mode: process multiple items with bounded concurrency.
    ///
    /// Returns an [`AgentBatchBuilder`] that inherits model/timeout/tools from
    /// this builder. Call `.for_each()` to map items to tasks, then `.execute()`.
    pub fn items<T>(self, items: Vec<T>, concurrency: usize) -> AgentBatchBuilder<'a, T> {
        AgentBatchBuilder {
            executor: self.executor,
            name: self.name,
            auth: self.auth,
            model: self.model,
            timeout: self.timeout,
            tools: self.tools,
            items,
            concurrency,
            mapper: None,
        }
    }

    /// Resolve the model string for task YAML, using the provider and config from the executor.
    fn resolve_model_string(&self) -> Option<String> {
        self.model.as_ref().map(|m| {
            let provider = self
                .executor
                .default_provider()
                .cloned()
                .unwrap_or_default();
            self.executor.model_config().resolve(m, &provider)
        })
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
    pub async fn execute(self) -> Result<AgentResult, PipelineError> {
        let model_str = self.resolve_model_string();
        let timeout_str = self.timeout.map(format_duration);

        let prompt = self
            .prompt
            .ok_or_else(|| PipelineError::Config(String::from("agent prompt is required")))?;

        let task_yaml = build_task_yaml(&TaskConfig {
            name: self.name.clone(),
            model: model_str,
            timeout: timeout_str,
            provider_id: None,
            max_concurrent: None,
            max_wait: None,
            max_retries: None,
            allowed_tools: self.tools,
        })?;

        // Resolve auth: builder override > executor default > empty.
        let auth = self.auth.or_else(|| self.executor.default_auth());
        let auth_env = auth
            .map(Auth::to_env)
            .transpose()?
            .unwrap_or_default();

        let env = merge_env(auth_env, None);

        // Dry-run: capture to disk.
        if let Some(dry_run_mutex) = self.executor.dry_run_config() {
            return execute_dry_run_capture(
                dry_run_mutex,
                &task_yaml,
                &prompt,
                self.expected_output.as_deref(),
                self.working_dir.as_deref(),
            );
        }

        // Execute via the shared remote bundle helper.
        execute_remote_bundle(
            self.executor.sdk_client(),
            self.executor.is_remote(),
            self.bundle_data.as_ref(),
            &task_yaml,
            &prompt,
            self.working_dir.as_deref(),
            self.expected_output.as_deref(),
            env,
        )
        .await
    }
}

/// Builder for batch agent invocations with bounded concurrency.
///
/// Created by [`AgentBuilder::items`]. Inherits model/timeout/tools from
/// the parent builder and applies them to each spawned task.
#[must_use]
pub struct AgentBatchBuilder<'a, T> {
    executor: &'a Executor,
    name: String,
    auth: Option<&'a Auth>,
    model: Option<Model>,
    timeout: Option<Duration>,
    tools: Option<String>,
    items: Vec<T>,
    concurrency: usize,
    mapper: Option<Box<dyn Fn(T) -> AgentTask + Send + Sync>>,
}

/// Alias for the shared result store used during batch execution.
type BatchResultStore = Arc<Mutex<Vec<Option<Result<AgentResult, PipelineError>>>>>;

/// Shared configuration extracted from the batch builder for use in pool tasks.
#[derive(Clone)]
struct BatchConfig {
    name: String,
    model: Option<String>,
    timeout: Option<String>,
    tools: Option<String>,
    default_auth_env: BTreeMap<String, String>,
}

impl<T: Send + 'static> AgentBatchBuilder<'_, T> {
    /// Set the mapping function that converts each item to an [`AgentTask`].
    pub fn for_each<F>(mut self, f: F) -> Self
    where
        F: Fn(T) -> AgentTask + Send + Sync + 'static,
    {
        self.mapper = Some(Box::new(f));
        self
    }

    /// Resolve the model string for task YAML, using the provider and config from the executor.
    fn resolve_model_string(&self) -> Option<String> {
        self.model.as_ref().map(|m| {
            let provider = self
                .executor
                .default_provider()
                .cloned()
                .unwrap_or_default();
            self.executor.model_config().resolve(m, &provider)
        })
    }

    /// Execute the batch: run all items through the pool with bounded concurrency.
    ///
    /// Each item is mapped to an [`AgentTask`] via the closure provided to `for_each()`.
    /// Returns one `Result<AgentResult>` per item.
    ///
    /// # Errors
    /// Returns an error if no `for_each` mapper was set.
    pub async fn execute(self) -> Result<Vec<Result<AgentResult, PipelineError>>, PipelineError> {
        let model_str = self.resolve_model_string();
        let timeout_str = self.timeout.map(format_duration);

        let mapper = self
            .mapper
            .ok_or_else(|| PipelineError::Config(String::from("batch requires a for_each mapper")))?;

        // Resolve default auth once for all tasks.
        let default_auth = self.auth.or_else(|| self.executor.default_auth());
        let default_auth_env = default_auth
            .map(Auth::to_env)
            .transpose()?
            .unwrap_or_default();

        let config = BatchConfig {
            name: self.name.clone(),
            model: model_str,
            timeout: timeout_str,
            tools: self.tools.clone(),
            default_auth_env,
        };

        // Map items to AgentTask + config pairs.
        let total = self.items.len();
        let tasks: Vec<(AgentTask, BatchConfig)> = self
            .items
            .into_iter()
            .map(|item| (mapper(item), config.clone()))
            .collect();

        // Dry-run mode: execute sequentially, capture to disk.
        if let Some(dry_run_mutex) = self.executor.dry_run_config() {
            let mut results = Vec::with_capacity(total);
            for (task, cfg) in &tasks {
                let result = execute_batch_task_dry_run(task, cfg, dry_run_mutex)?;
                results.push(Ok(result));
            }
            return Ok(results);
        }

        // Real execution: use run_pool with result capture via Arc<Mutex<Vec>>.
        let results_store: BatchResultStore =
            Arc::new(Mutex::new((0..total).map(|_| None).collect()));

        let client = self.executor.sdk_client().clone();
        let remote = self.executor.is_remote();

        let results_for_pool = Arc::clone(&results_store);
        let _pool_outcomes = run_pool(tasks, self.concurrency, move |pair, index| {
            let client = client.clone();
            let store = Arc::clone(&results_for_pool);
            async move {
                let (task, cfg) = pair;
                let result = execute_single_task(&task, &cfg, &client, remote).await;
                {
                    let mut guard = store
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(slot) = guard.get_mut(index) {
                        *slot = Some(result);
                    }
                }
                Ok(())
            }
        })
        .await;

        // Extract results from the store. All pool tasks have completed at this
        // point, so we are the sole owner of the Arc.
        let inner = Arc::try_unwrap(results_store)
            .map_err(|_| PipelineError::Other(String::from("batch results Arc still shared after pool completion")))?
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        Ok(inner
            .into_iter()
            .map(|opt| opt.unwrap_or_else(|| Err(PipelineError::Other(String::from("batch task result missing")))))
            .collect())
    }
}

/// Write a dry-run capture for a single invocation.
fn execute_dry_run_capture(
    dry_run_mutex: &std::sync::Mutex<crate::executor::DryRunConfig>,
    task_yaml: &str,
    prompt: &str,
    expected_output: Option<&Path>,
    working_dir: Option<&Path>,
) -> Result<AgentResult, PipelineError> {
    let mut guard = dry_run_mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let step_name = extract_step_name(task_yaml);
    let index = guard.counter;
    guard.counter = guard.counter.saturating_add(1);

    let capture_dir = guard.base_dir.join(&step_name).join(index.to_string());
    drop(guard); // Release lock before I/O.

    std::fs::create_dir_all(&capture_dir)?;
    std::fs::write(capture_dir.join("prompt.md"), prompt)?;
    std::fs::write(capture_dir.join("task.yaml"), task_yaml)?;

    let meta = serde_json::json!({
        "expectedOutput": expected_output.map(|p| p.display().to_string()),
        "workingDirectory": working_dir.map(|p| p.display().to_string()),
    });
    std::fs::write(
        capture_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| {
            PipelineError::Other(format!("failed to serialize meta: {e}"))
        })?,
    )?;

    tracing::info!("[dry-run] Captured to {}", capture_dir.display());
    Ok(AgentResult {
        success: true,
        output: String::from("(dry-run)"),
    })
}

/// Write a dry-run capture for a batch task.
fn execute_batch_task_dry_run(
    task: &AgentTask,
    config: &BatchConfig,
    dry_run_mutex: &std::sync::Mutex<crate::executor::DryRunConfig>,
) -> Result<AgentResult, PipelineError> {
    let prompt = task
        .prompt
        .as_ref()
        .ok_or_else(|| PipelineError::Config(String::from("agent task prompt is required")))?;

    let task_yaml = build_task_yaml(&TaskConfig {
        name: config.name.clone(),
        model: config.model.clone(),
        timeout: config.timeout.clone(),
        provider_id: None,
        max_concurrent: None,
        max_wait: None,
        max_retries: None,
        allowed_tools: config.tools.clone(),
    })?;

    execute_dry_run_capture(
        dry_run_mutex,
        &task_yaml,
        prompt,
        task.expected_output.as_deref(),
        task.working_dir.as_deref(),
    )
}

/// Execute a remote bundle workflow: upload, submit, download outputs, cleanup.
///
/// Shared by both [`AgentBuilder::execute`] and [`execute_single_task`] to avoid
/// duplicating the upload/submit/download/cleanup sequence.
#[allow(clippy::too_many_arguments)] // reason: flat param list avoids an intermediate struct for a private helper
async fn execute_remote_bundle(
    client: &shedul3r_rs_sdk::Client,
    remote: bool,
    bundle: Option<&Bundle>,
    task_yaml: &str,
    prompt: &str,
    working_dir: Option<&Path>,
    expected_output: Option<&Path>,
    env: Option<BTreeMap<String, String>>,
) -> Result<AgentResult, PipelineError> {
    // Upload bundle when remote mode is enabled and a bundle is present.
    let bundle_handle = if remote {
        if let Some(bundle) = bundle {
            let file_refs: Vec<(&str, &[u8])> = bundle
                .files()
                .iter()
                .map(|(name, content)| (name.as_str(), content.as_slice()))
                .collect();
            Some(client.upload_bundle(&file_refs).await?)
        } else {
            None
        }
    } else {
        None
    };

    // Use remote path as working directory when a bundle was uploaded.
    let working_directory = if let Some(ref handle) = bundle_handle {
        Some(handle.remote_path.clone())
    } else {
        working_dir.map(|p| p.display().to_string())
    };

    let payload = TaskPayload {
        task: String::from(task_yaml),
        input: String::from(prompt),
        working_directory,
        environment: env,
    };

    // Wrap execution in a block that always cleans up the bundle.
    let execution_result = async {
        let result = if let Some(expected) = expected_output {
            client
                .submit_task_with_recovery(&payload, expected)
                .await?
        } else {
            client.submit_task(&payload).await?
        };

        // Download expected outputs from remote bundle.
        if let Some(ref handle) = bundle_handle {
            if let Some(bundle) = bundle {
                for output_path in bundle.expected_output_paths() {
                    let bytes = client
                        .download_file(&handle.id, output_path)
                        .await?;

                    // Write downloaded file to the local working directory or temp.
                    let local_dir = working_dir
                        .map_or_else(std::env::temp_dir, std::path::Path::to_path_buf);
                    let local_path = local_dir.join(output_path);
                    if let Some(parent) = local_path.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    tokio::fs::write(&local_path, &bytes).await?;
                }
            }
        }

        Ok::<AgentResult, PipelineError>(AgentResult {
            success: result.success,
            output: result.output,
        })
    }
    .await;

    // Always clean up remote bundle, regardless of success/failure.
    if let Some(ref handle) = bundle_handle {
        if let Err(e) = client.delete_bundle(&handle.id).await {
            tracing::warn!("failed to delete remote bundle {}: {e}", handle.id);
        }
    }

    execution_result
}

/// Execute a single task via the SDK client, with bundle cleanup on failure.
async fn execute_single_task(
    task: &AgentTask,
    config: &BatchConfig,
    client: &shedul3r_rs_sdk::Client,
    remote: bool,
) -> Result<AgentResult, PipelineError> {
    let prompt = task
        .prompt
        .as_ref()
        .ok_or_else(|| PipelineError::Config(String::from("agent task prompt is required")))?;

    let task_yaml = build_task_yaml(&TaskConfig {
        name: config.name.clone(),
        model: config.model.clone(),
        timeout: config.timeout.clone(),
        provider_id: None,
        max_concurrent: None,
        max_wait: None,
        max_retries: None,
        allowed_tools: config.tools.clone(),
    })?;

    // Resolve auth: task override > batch default.
    let auth_env = if let Some(ref auth) = task.auth {
        auth.to_env()?
    } else {
        config.default_auth_env.clone()
    };

    let env = merge_env(auth_env, None);

    // Execute via the shared remote bundle helper.
    execute_remote_bundle(
        client,
        remote,
        task.bundle_data.as_ref(),
        &task_yaml,
        prompt,
        task.working_dir.as_deref(),
        task.expected_output.as_deref(),
        env,
    )
    .await
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
    #[allow(clippy::unwrap_used)] // reason: test assertion on known-Err value
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

    #[test]
    fn agent_task_builder_chain() {
        let task = AgentTask::new()
            .prompt("hello")
            .working_dir(Path::new("/tmp"))
            .expected_output(Path::new("/tmp/out.txt"))
            .auth(Auth::ApiKey(String::from("sk-test")));

        assert_eq!(
            task.prompt.as_deref(),
            Some("hello"),
            "prompt should be set"
        );
        assert_eq!(
            task.working_dir.as_deref(),
            Some(Path::new("/tmp")),
            "working_dir should be set"
        );
        assert_eq!(
            task.expected_output.as_deref(),
            Some(Path::new("/tmp/out.txt")),
            "expected_output should be set"
        );
        assert!(task.auth.is_some(), "auth should be set");
    }

    #[test]
    fn agent_task_default_is_empty() {
        let task = AgentTask::new();
        assert!(task.prompt.is_none(), "prompt should default to None");
        assert!(
            task.working_dir.is_none(),
            "working_dir should default to None"
        );
        assert!(
            task.expected_output.is_none(),
            "expected_output should default to None"
        );
        assert!(task.bundle_data.is_none(), "bundle should default to None");
        assert!(task.auth.is_none(), "auth should default to None");
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)] // reason: test assertion
    async fn batch_dry_run_produces_correct_count() {
        let executor = Executor::with_defaults()
            .unwrap()
            .with_dry_run(PathBuf::from("/tmp/pipelin3r-batch-test"));

        let items: Vec<String> = vec![
            String::from("item_a"),
            String::from("item_b"),
            String::from("item_c"),
        ];

        let results = executor
            .agent("test-batch")
            .model(Model::Sonnet4_6)
            .items(items, 2)
            .for_each(|item| AgentTask::new().prompt(&format!("process {item}")))
            .execute()
            .await
            .unwrap();

        assert_eq!(results.len(), 3, "should produce one result per item");
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok(), "item {i} should succeed in dry-run");
        }

        // Clean up test artifacts.
        let _ = std::fs::remove_dir_all("/tmp/pipelin3r-batch-test");
    }

    #[tokio::test]
    async fn batch_without_mapper_fails() {
        let executor = Executor::with_defaults().unwrap_or_else(|_| {
            Executor::new(&shedul3r_rs_sdk::ClientConfig::default())
                .unwrap_or_else(|_| std::process::abort())
        });

        let items: Vec<u32> = vec![1, 2];
        let result = executor
            .agent("test")
            .items(items, 1)
            .execute()
            .await;

        assert!(
            result.is_err(),
            "should fail without for_each mapper"
        );
    }
}
