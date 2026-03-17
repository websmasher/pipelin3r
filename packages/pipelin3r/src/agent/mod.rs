//! Agent builder for single and batch LLM agent invocations.

mod execute;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::auth::{Auth, EnvironmentMap, merge_env};
use crate::error::PipelineError;
use crate::executor::Executor;
use crate::model::{Model, Tool};
use crate::pool::run_pool;
use crate::task::{TaskConfig, build_task_yaml};

use execute::{
    count_batch_outcomes, execute_batch_task_dry_run, execute_dry_run_capture, execute_single_task,
    execute_with_work_dir, format_duration, is_partial_failure, validate_work_dir,
};

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
            Err(PipelineError::AgentFailed {
                message: self.output.clone(),
            })
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
    /// Work directory path for this task.
    pub(crate) work_dir: Option<PathBuf>,
    /// Transport hint: which files to download back from remote after execution.
    pub(crate) expected_outputs: Vec<String>,
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

    /// Set the work directory for this task.
    pub fn work_dir(mut self, path: &Path) -> Self {
        self.work_dir = Some(path.to_path_buf());
        self
    }

    /// Set the expected output file paths (transport hint for remote mode).
    ///
    /// When the shedul3r server is remote, these paths (relative to the work
    /// directory) are downloaded back after execution completes.
    pub fn expect_outputs(mut self, paths: &[&str]) -> Self {
        self.expected_outputs = paths.iter().map(|s| String::from(*s)).collect();
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
    work_dir: Option<PathBuf>,
    expected_outputs: Vec<String>,
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
            work_dir: None,
            expected_outputs: Vec::new(),
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

    /// Set the allowed tools.
    pub fn tools(mut self, tools: &[Tool]) -> Self {
        let joined: String = tools
            .iter()
            .enumerate()
            .fold(String::new(), |mut acc, (i, t)| {
                if i > 0 {
                    acc.push(',');
                }
                acc.push_str(t.as_str());
                acc
            });
        self.tools = Some(joined);
        self
    }

    /// Set the prompt text to send to the agent.
    pub fn prompt(mut self, text: &str) -> Self {
        self.prompt = Some(String::from(text));
        self
    }

    /// Set the work directory for the agent.
    ///
    /// For local shedul3r servers (localhost), the path is passed directly.
    /// For remote servers, the directory contents are uploaded as a bundle
    /// and downloaded back after execution.
    pub fn work_dir(mut self, path: &Path) -> Self {
        self.work_dir = Some(path.to_path_buf());
        self
    }

    /// Set the expected output file paths (transport hint for remote mode).
    ///
    /// When the shedul3r server is remote, these paths (relative to the work
    /// directory) are downloaded back after execution completes.
    pub fn expect_outputs(mut self, paths: &[&str]) -> Self {
        self.expected_outputs = paths.iter().map(|s| String::from(*s)).collect();
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
        // Validate work_dir before any work happens.
        if let Some(ref dir) = self.work_dir {
            validate_work_dir(dir)?;
        }

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
        let auth_env = auth.map(Auth::to_env).transpose()?.unwrap_or_default();

        let env = merge_env(auth_env, None);

        // Dry-run: capture to disk.
        if let Some(dry_run_mutex) = self.executor.dry_run_config() {
            return execute_dry_run_capture(
                dry_run_mutex,
                &task_yaml,
                &prompt,
                self.work_dir.as_deref(),
                env.as_ref(),
            );
        }

        // Execute via the work-dir transport helper.
        execute_with_work_dir(
            self.executor.sdk_client(),
            !self.executor.is_local(),
            &task_yaml,
            &prompt,
            self.work_dir.as_deref(),
            &self.expected_outputs,
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
    #[allow(
        clippy::type_complexity,
        reason = "closure type for item-to-task mapping is inherently complex"
    )]
    mapper: Option<Box<dyn Fn(T) -> AgentTask + Send + Sync>>,
}

/// Alias for the shared result store used during batch execution.
#[allow(
    clippy::disallowed_types,
    reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
)]
type BatchResultStore = Arc<std::sync::Mutex<Vec<Option<Result<AgentResult, PipelineError>>>>>;

/// Shared configuration extracted from the batch builder for use in pool tasks.
#[derive(Clone)]
struct BatchConfig {
    name: String,
    model: Option<String>,
    timeout: Option<String>,
    tools: Option<String>,
    default_auth_env: EnvironmentMap,
    is_local: bool,
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
    #[allow(
        clippy::disallowed_methods,
        reason = "canonicalize needed for duplicate work_dir detection across symlinks"
    )]
    #[allow(
        clippy::too_many_lines,
        reason = "batch orchestration has sequential phases that are clearer kept together"
    )]
    #[allow(
        clippy::disallowed_types,
        reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
    )]
    pub async fn execute(self) -> Result<Vec<Result<AgentResult, PipelineError>>, PipelineError> {
        let model_str = self.resolve_model_string();
        let timeout_str = self.timeout.map(format_duration);

        let mapper = self.mapper.ok_or_else(|| {
            PipelineError::Config(String::from("batch requires a for_each mapper"))
        })?;

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
            is_local: self.executor.is_local(),
        };

        // Map items to AgentTask + config pairs.
        let total = self.items.len();
        let tasks: Vec<_> = self
            .items
            .into_iter()
            .map(|item| (mapper(item), config.clone()))
            .collect();

        // Validate no two tasks share the same work_dir.
        // Canonicalize paths to catch equivalences like `/tmp/a/../work` == `/tmp/work`.
        {
            let mut seen_dirs = BTreeSet::new();
            for (task, _) in &tasks {
                if let Some(ref dir) = task.work_dir {
                    let canonical = crate::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
                    if !seen_dirs.insert(canonical) {
                        return Err(PipelineError::Config(format!(
                            "duplicate work_dir in batch: {}",
                            dir.display()
                        )));
                    }
                }
            }
        }

        // Dry-run mode: execute sequentially, capture to disk.
        // Collect per-item results like real mode instead of propagating errors.
        if let Some(dry_run_mutex) = self.executor.dry_run_config() {
            let mut results: Vec<_> = Vec::with_capacity(total);
            for (task, cfg) in &tasks {
                results.push(execute_batch_task_dry_run(task, cfg, dry_run_mutex));
            }
            return Ok(results);
        }

        // Real execution: use run_pool with result capture via Arc<Mutex<Vec>>.
        let results_store: BatchResultStore =
            Arc::new(std::sync::Mutex::new((0..total).map(|_| None).collect()));

        let client = self.executor.sdk_client().clone();

        let results_for_pool = Arc::clone(&results_store);
        let _pool_outcomes = run_pool(tasks, self.concurrency, move |pair, index| {
            let client = client.clone();
            let store = Arc::clone(&results_for_pool);
            async move {
                let (task, cfg) = pair;
                let result = execute_single_task(&task, &cfg, &client).await;
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
            .map_err(|_| {
                PipelineError::Other(String::from(
                    "batch results Arc still shared after pool completion",
                ))
            })?
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let results: Vec<_> = inner
            .into_iter()
            .map(|opt| {
                opt.unwrap_or_else(|| {
                    Err(PipelineError::AgentFailed {
                        message: String::from("batch task result missing"),
                    })
                })
            })
            .collect();

        // Check for partial failures and report via BatchPartialFailure if needed.
        let (succeeded, failed) = count_batch_outcomes(&results);

        if is_partial_failure(succeeded, failed) {
            tracing::warn!(
                "Batch partial failure: {succeeded} succeeded, {failed} failed out of {total}"
            );
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests;
