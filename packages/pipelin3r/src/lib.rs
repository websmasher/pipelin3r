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
/// Typed LLM model and provider selection.
pub mod model;
/// Bounded async concurrency pool.
pub mod pool;
/// Two-phase template filler.
pub mod template;
/// Pure function transforms (stub).
pub mod transform;
/// Utility functions for processing LLM output.
pub mod utils;

// Private: task YAML builder used by agent.rs.
pub(crate) mod task;

pub use agent::{AgentConfig, AgentResult, RetryConfig};
pub use auth::{Auth, EnvironmentMap};
pub use bundle_dir::BundleDir;
pub use command::{CommandBuilder, CommandResult};
pub use error::PipelineError;
pub use executor::Executor;
pub use model::{Model, ModelConfig, Provider, Tool};
pub use pool::{run_pool, run_pool_map};
pub use template::TemplateFiller;
pub use transform::{TransformBuilder, TransformResult};
pub use utils::{chunk_by_size, parse_labeled_fields, strip_code_fences, strip_preamble};
