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

        let result = if self.executor.is_local() {
            // Local: use base_dir directly.
            let config = AgentConfig {
                work_dir: Some(self.base_dir.clone()),
                expect_outputs: step.outputs.clone(),
                ..step.config
            };
            self.executor.run_agent(&config).await?
        } else {
            // Remote: create a temp dir with only the input files,
            // run the agent there, copy outputs back to base_dir.
            self.run_agent_remote(&step).await?
        };

        // Verify outputs exist.
        if result.success {
            for output in &step.outputs {
                let path = self.base_dir.join(output);
                if !path.is_file() {
                    tracing::warn!("expected output not found: {}", output);
                }
            }
        }

        Ok(result)
    }

    /// Remote agent execution: delegate to shared helper.
    async fn run_agent_remote(&self, step: &AgentStep) -> Result<AgentResult, PipelineError> {
        run_agent_with_temp_dir(&self.executor, &self.base_dir, step.clone()).await
    }

    /// Run a batch of agent steps with bounded concurrency.
    ///
    /// For each item, calls the mapper to produce an [`AgentStep`], then
    /// runs all steps concurrently with the given concurrency limit.
    ///
    /// Returns per-item results paired with the original items.
    #[allow(
        clippy::too_many_lines,
        reason = "batch orchestration with remote/local branching"
    )]
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

                let result = if executor.is_local() {
                    let config = AgentConfig {
                        work_dir: Some(base_dir.clone()),
                        expect_outputs: step.outputs.clone(),
                        ..step.config
                    };
                    executor.run_agent(&config).await
                } else {
                    // Remote: temp dir with only inputs
                    run_agent_with_temp_dir(&executor, &base_dir, step).await
                };

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

/// Run an agent in a temp dir with only declared inputs, copy outputs back.
///
/// Used by both `run_agent` (single) and `run_agent_batch` (per-item) for
/// remote execution to avoid uploading the entire `base_dir`.
async fn run_agent_with_temp_dir(
    executor: &Executor,
    base_dir: &Path,
    step: AgentStep,
) -> Result<AgentResult, PipelineError> {
    let temp_dir = tempfile::tempdir()
        .map_err(|e| PipelineError::Transport(format!("failed to create temp dir: {e}")))?;

    // Copy input files into temp dir.
    for input in &step.inputs {
        let src = base_dir.join(input);
        let dst = temp_dir.path().join(input);
        if let Some(parent) = dst.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        let _ = crate::fs::copy(&src, &dst)?;
    }

    let config = AgentConfig {
        work_dir: Some(temp_dir.path().to_path_buf()),
        expect_outputs: step.outputs.clone(),
        ..step.config
    };

    let result = executor.run_agent(&config).await?;

    // Copy outputs back to base_dir.
    if result.success {
        for output in &step.outputs {
            let src = temp_dir.path().join(output);
            if src.is_file() {
                let dst = base_dir.join(output);
                if let Some(parent) = dst.parent() {
                    crate::fs::create_dir_all(parent)?;
                }
                let _ = crate::fs::copy(&src, &dst)?;
            }
        }
    }

    Ok(result)
}
