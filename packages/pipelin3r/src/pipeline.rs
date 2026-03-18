//! Pipeline context for managing file routing between steps.
//!
//! [`PipelineContext`] tracks a base directory and handles input/output
//! file routing for each step. For local execution, files are read/written
//! directly. For remote execution, only declared `inputs` are uploaded
//! and only declared `outputs` are downloaded back.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::{AgentConfig, AgentResult};
use crate::error::PipelineError;
use crate::executor::Executor;
use crate::pool::run_pool_map;

/// A pipeline step that runs an LLM agent with declared inputs and outputs.
///
/// `inputs` are relative paths (from the pipeline base dir) that the agent
/// needs to read. For remote execution, only these files are uploaded.
///
/// `outputs` are relative paths that the agent will write. For remote
/// execution, these files are downloaded back after the agent finishes.
#[derive(Debug, Clone)]
pub struct AgentStep {
    /// Agent configuration (name, prompt, model, tools, etc.).
    ///
    /// `work_dir` and `expect_outputs` are set automatically by the context —
    /// do not set them on the config.
    pub config: AgentConfig,
    /// Files this step reads (relative to pipeline base dir).
    pub inputs: Vec<String>,
    /// Files this step produces (relative to pipeline base dir).
    pub outputs: Vec<String>,
}

/// Pipeline context that manages file routing between steps.
///
/// Wraps an [`Executor`] and a base directory. Each step declares its
/// inputs and outputs; the context handles transport (local path or
/// remote bundle upload/download).
pub struct PipelineContext {
    executor: Arc<Executor>,
    base_dir: PathBuf,
}

impl PipelineContext {
    /// Create a new pipeline context.
    ///
    /// `base_dir` is the root directory where all step inputs/outputs live.
    /// All paths in `AgentStep::inputs` and `AgentStep::outputs` are
    /// relative to this directory.
    pub const fn new(executor: Arc<Executor>, base_dir: PathBuf) -> Self {
        Self { executor, base_dir }
    }

    /// Get the base directory.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Get a reference to the executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Run a single agent step with file routing.
    ///
    /// 1. Verifies all `inputs` exist on disk
    /// 2. Sets `work_dir` and `expect_outputs` on the config
    /// 3. For remote: uploads only `inputs` files, downloads `outputs` after
    /// 4. Verifies all `outputs` exist after execution
    ///
    /// # Errors
    ///
    /// Returns an error if inputs are missing, the agent fails, or outputs
    /// are not produced.
    pub async fn run_agent(&self, step: AgentStep) -> Result<AgentResult, PipelineError> {
        // Verify inputs exist.
        for input in &step.inputs {
            let path = self.base_dir.join(input);
            if !path.is_file() {
                return Err(PipelineError::Config(format!(
                    "step '{}': input file not found: {}",
                    step.config.name, input
                )));
            }
        }

        // Build the agent config with work_dir and expect_outputs.
        let config = AgentConfig {
            work_dir: Some(self.base_dir.clone()),
            expect_outputs: step.outputs.clone(),
            ..step.config
        };

        let result = self.executor.run_agent(&config).await?;

        // Verify outputs exist (for local execution — remote downloads
        // are handled by run_agent internally via expect_outputs).
        if result.success {
            for output in &step.outputs {
                let path = self.base_dir.join(output);
                if !path.is_file() {
                    tracing::warn!(
                        "step '{}': expected output not found: {}",
                        config.name,
                        output
                    );
                }
            }
        }

        Ok(result)
    }

    /// Run a batch of agent steps with bounded concurrency.
    ///
    /// For each item, calls the mapper to produce an [`AgentStep`], then
    /// runs all steps concurrently with the given concurrency limit.
    ///
    /// Returns per-item results paired with the original items.
    pub async fn run_agent_batch<T, F>(
        &self,
        items: Vec<T>,
        concurrency: usize,
        mapper: F,
    ) -> Vec<(T, Result<AgentResult, PipelineError>)>
    where
        T: Send + 'static,
        F: Fn(&T) -> AgentStep + Send + Sync + Clone + 'static,
    {
        let total = items.len();
        let executor = Arc::clone(&self.executor);
        let base_dir = self.base_dir.clone();

        run_pool_map(items, concurrency, total, move |item, idx, total| {
            let executor = Arc::clone(&executor);
            let base_dir = base_dir.clone();
            let mapper = mapper.clone();

            async move {
                let step = mapper(&item);
                let step_name = step.config.name.clone();

                tracing::info!(
                    "[{}/{}] Running {}",
                    idx.saturating_add(1),
                    total,
                    step_name
                );

                // Verify inputs exist.
                for input in &step.inputs {
                    let path = base_dir.join(input);
                    if !path.is_file() {
                        return (
                            item,
                            Err(PipelineError::Config(format!(
                                "step '{step_name}': input file not found: {input}"
                            ))),
                        );
                    }
                }

                let config = AgentConfig {
                    work_dir: Some(base_dir.clone()),
                    expect_outputs: step.outputs.clone(),
                    ..step.config
                };

                let result = executor.run_agent(&config).await;

                if let Ok(ref r) = result {
                    if r.success {
                        tracing::info!("[{}/{}] OK {}", idx.saturating_add(1), total, step_name);
                    } else {
                        tracing::warn!(
                            "[{}/{}] FAILED {}",
                            idx.saturating_add(1),
                            total,
                            step_name
                        );
                    }
                }

                (item, result)
            }
        })
        .await
    }

    /// Run a programmatic (non-agent) step.
    ///
    /// Calls the closure with the base directory path. The closure does
    /// whatever filesystem work is needed. No remote transport — local only.
    ///
    /// # Errors
    ///
    /// Returns whatever error the closure returns.
    pub fn run_local<F>(&self, name: &str, f: F) -> Result<(), PipelineError>
    where
        F: FnOnce(&Path) -> Result<(), PipelineError>,
    {
        tracing::info!("[{name}] Running local step");
        f(&self.base_dir)
    }
}
