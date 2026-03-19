//! Pipeline orchestration for LLM-powered workflows.
//!
//! `pipelin3r` provides a config-struct API for orchestrating LLM agent
//! invocations, shell commands, and data transforms in a pipeline. It wraps
//! the `shedul3r-rs-sdk` HTTP client with authentication, dry-run capture,
//! and bounded-concurrency batch execution.

/// Agent configuration and execution for LLM invocations.
pub mod agent;
/// Per-invocation authentication.
pub mod auth;
/// Bundle packaging for file transfer (internal transport mechanism).
pub(crate) mod bundle;
/// RAII guard for ephemeral bundle work directories.
pub mod bundle_dir;
/// Shell command execution.
pub mod command;
/// Typed error enum.
pub mod error;
/// Pipeline executor (SDK client + auth + dry-run).
pub mod executor;
/// Centralized filesystem operations.
pub(crate) mod fs;
/// Image generation via the OpenRouter API.
pub mod image_gen;
/// Typed LLM model and provider selection.
pub mod model;
/// Pipeline context for step orchestration with file routing.
pub mod pipeline;
/// Bounded async concurrency pool.
pub mod pool;
/// Two-phase template filler.
pub mod template;
/// Pure function transforms (stub).
pub mod transform;
/// Utility functions for processing LLM output.
pub mod utils;
/// Validate-and-fix loop for iterative convergence.
pub mod validate;
/// Doer-breaker-fixer pattern for verified LLM pipeline steps.
pub mod verified;

// Private: task YAML builder used by agent.rs.
pub(crate) mod task;

pub use agent::{AgentConfig, AgentResult, RetryConfig};
pub use auth::{Auth, EnvironmentMap};
pub use bundle_dir::BundleDir;
pub use command::{CommandConfig, CommandResult, run_command};
pub use error::PipelineError;
pub use executor::{Executor, RemoteCommandConfig};
pub use image_gen::{
    AspectRatio, ImageGenConfig, ImageGenHttpConfig, ImageGenResult, ImageModel, RefImage,
    RefImageRole, generate_image,
};
pub use model::{Model, ModelConfig, Provider, Tool};
pub use pipeline::{AgentStep, PipelineContext};
pub use pool::{run_pool, run_pool_map};
pub use template::TemplateFiller;
pub use transform::{TransformBuilder, TransformResult};
pub use utils::{chunk_by_size, parse_labeled_fields, strip_code_fences, strip_preamble};
pub use validate::{
    RemediationAction, ValidateConfig, ValidateResult, ValidationFinding, ValidationReport,
    validate_and_fix,
};
pub use verified::{
    Breaker, PromptedStep, Var, VerifiedStep, VerifiedStepResult, run_verified_step,
    run_verified_step_batch,
};
