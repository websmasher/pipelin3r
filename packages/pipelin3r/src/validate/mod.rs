//! Validate-and-fix loop for iterative convergence.
//!
//! Runs a user-supplied validator, then applies remediation actions from a
//! user-supplied strategy until the validator passes or iterations are exhausted.

mod action;
mod report;

pub use action::RemediationAction;
pub use report::{ValidationFinding, ValidationReport};

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use crate::agent::{AgentConfig, AgentResult};
use crate::error::PipelineError;
use crate::executor::Executor;

/// Configuration for a validate-and-fix loop.
#[derive(Debug, Clone)]
pub struct ValidateConfig {
    /// Name of this validation loop (for logging).
    pub name: String,
    /// Default work directory for validation and fix agents.
    pub work_dir: PathBuf,
    /// Maximum number of validate-fix iterations before giving up.
    pub max_iterations: u32,
    /// Default agent config for `AgentFix` remediation actions.
    /// The `name` and `prompt` fields are overridden per-action.
    pub fix_agent_defaults: AgentConfig,
}

/// Default maximum iterations when not specified.
const DEFAULT_MAX_ITERATIONS: u32 = 3;

impl ValidateConfig {
    /// Create a new validation config with sensible defaults.
    ///
    /// Sets `max_iterations` to 3 and creates minimal `fix_agent_defaults`.
    #[must_use]
    pub fn new(name: impl Into<String>, work_dir: PathBuf) -> Self {
        Self {
            name: name.into(),
            work_dir: work_dir.clone(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            fix_agent_defaults: AgentConfig {
                work_dir: Some(work_dir),
                ..AgentConfig::new("fix", "")
            },
        }
    }
}

/// Result of a validate-and-fix loop.
#[derive(Debug)]
pub struct ValidateResult {
    /// Whether the validator passed within the iteration budget.
    pub converged: bool,
    /// Number of iterations executed (including the final one).
    pub iterations: u32,
    /// The report from the final validation run.
    pub final_report: ValidationReport,
    /// Reports from all iterations, in order.
    pub history: Vec<ValidationReport>,
}

impl ValidateResult {
    /// Return a reference to self if converged, or an error if not.
    ///
    /// # Errors
    /// Returns [`PipelineError::ValidationExhausted`] if the loop did not converge.
    pub fn require_converged(&self) -> Result<&Self, PipelineError> {
        if self.converged {
            Ok(self)
        } else {
            let remaining: Vec<String> = self
                .final_report
                .findings
                .iter()
                .map(|f| {
                    let mut s = String::from("[");
                    s.push_str(&f.tag);
                    s.push_str("] ");
                    s.push_str(&f.message);
                    s
                })
                .collect();
            let remaining_desc = if remaining.is_empty() {
                self.final_report
                    .raw_output
                    .clone()
                    .unwrap_or_else(|| String::from("(no details)"))
            } else {
                remaining.join("; ")
            };
            Err(PipelineError::ValidationExhausted {
                name: String::from("validate"),
                iterations: self.iterations,
                remaining_errors: remaining_desc,
            })
        }
    }
}

/// Run a validate-and-fix loop.
///
/// 1. Run the validator against `config.work_dir`.
/// 2. If passed, return converged result.
/// 3. If iterations exhausted, return non-converged result.
/// 4. Get remediation actions from the strategy.
/// 5. If no actions, return non-converged (strategy says nothing is fixable).
/// 6. Execute actions sequentially (`AgentFix` -> `run_agent`, `FunctionFix` -> call, `Skip` -> log).
/// 7. Increment iteration, go to 1.
///
/// # Errors
/// Returns an error if the validator or any remediation action fails with an
/// unrecoverable error (as opposed to a validation failure, which is expected).
pub async fn validate_and_fix<V, S>(
    executor: &Executor,
    config: &ValidateConfig,
    validator: V,
    strategy: S,
) -> Result<ValidateResult, PipelineError>
where
    V: Fn(
            &Path,
        )
            -> Pin<Box<dyn Future<Output = Result<ValidationReport, PipelineError>> + Send + '_>>
        + Send
        + Sync,
    S: Fn(&ValidationReport, u32) -> Vec<RemediationAction> + Send + Sync,
{
    let mut history: Vec<ValidationReport> = Vec::new();
    let mut iteration: u32 = 0;

    loop {
        // Run the validator.
        let report = validator(&config.work_dir).await?;
        let passed = report.passed;
        history.push(report);

        iteration = iteration.saturating_add(1);

        if passed {
            // Converged — the last report in history is the passing one.
            let final_report = history
                .last()
                .cloned()
                .unwrap_or_else(ValidationReport::pass);
            return Ok(ValidateResult {
                converged: true,
                iterations: iteration,
                final_report,
                history,
            });
        }

        // Check iteration budget.
        if iteration >= config.max_iterations {
            let final_report = history
                .last()
                .cloned()
                .unwrap_or_else(ValidationReport::pass);
            return Ok(ValidateResult {
                converged: false,
                iterations: iteration,
                final_report,
                history,
            });
        }

        // Get remediation actions from the strategy.
        let last_report = history
            .last()
            .ok_or_else(|| PipelineError::Other(String::from("empty history after validation")))?;
        let actions = strategy(last_report, iteration);

        if actions.is_empty() {
            // Strategy has no actions — nothing fixable.
            let final_report = last_report.clone();
            return Ok(ValidateResult {
                converged: false,
                iterations: iteration,
                final_report,
                history,
            });
        }

        // Execute remediation actions sequentially.
        for action in actions {
            let _ = execute_action(executor, config, action).await?;
        }
    }
}

/// Execute a single remediation action.
async fn execute_action(
    executor: &Executor,
    config: &ValidateConfig,
    action: RemediationAction,
) -> Result<Option<AgentResult>, PipelineError> {
    match action {
        RemediationAction::AgentFix {
            prompt,
            work_dir_override,
        } => {
            let work_dir = work_dir_override.unwrap_or_else(|| config.work_dir.clone());
            let agent_config = AgentConfig {
                name: format!("{}-fix", config.name),
                prompt,
                work_dir: Some(work_dir),
                ..config.fix_agent_defaults.clone()
            };
            let result = executor.run_agent(&agent_config).await?;
            Ok(Some(result))
        }
        RemediationAction::FunctionFix(func) => {
            let fut = func();
            fut.await?;
            Ok(None)
        }
        RemediationAction::Skip { reason } => {
            tracing::info!(
                name = %config.name,
                reason = %reason,
                "skipping remediation action"
            );
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests;
