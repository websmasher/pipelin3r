//! Remediation actions for the validate-and-fix loop.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use crate::error::PipelineError;

/// A boxed async closure that returns a `Result<(), PipelineError>`.
pub type AsyncFixFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), PipelineError>> + Send>> + Send>;

/// An action to take in response to a validation failure.
///
/// The strategy function returns a list of these actions, which the
/// validate-and-fix loop executes sequentially before re-validating.
pub enum RemediationAction {
    /// Run an agent with the given prompt to fix the issue.
    AgentFix {
        /// Prompt describing what the agent should fix.
        prompt: String,
        /// Optional work directory override (defaults to `ValidateConfig::work_dir`).
        work_dir_override: Option<PathBuf>,
    },
    /// Run an arbitrary async function to fix the issue.
    FunctionFix(AsyncFixFn),
    /// Skip this finding with a reason (logged but no action taken).
    Skip {
        /// Why this finding is being skipped.
        reason: String,
    },
}

// Manual Debug impl because FnOnce is not Debug.
impl std::fmt::Debug for RemediationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentFix {
                prompt,
                work_dir_override,
            } => f
                .debug_struct("AgentFix")
                .field("prompt", prompt)
                .field("work_dir_override", work_dir_override)
                .finish(),
            Self::FunctionFix(_) => f.debug_tuple("FunctionFix").field(&"<closure>").finish(),
            Self::Skip { reason } => f.debug_struct("Skip").field("reason", reason).finish(),
        }
    }
}
