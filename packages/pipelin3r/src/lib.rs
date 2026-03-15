//! Pipeline orchestration for LLM-powered workflows.
//!
//! `pipelin3r` provides a high-level builder API for orchestrating LLM agent
//! invocations, shell commands, and data transforms in a pipeline. It wraps
//! the `shedul3r-rs-sdk` HTTP client with authentication, dry-run capture,
//! and bounded-concurrency batch execution.

/// Per-invocation authentication.
pub mod auth;
/// Agent builder for single and batch LLM invocations.
pub mod agent;
/// Bundle packaging for file transfer.
pub mod bundle;
/// Shell command execution.
pub mod command;
/// Pipeline executor (SDK client + auth + dry-run).
pub mod executor;
/// Typed LLM model and provider selection.
pub mod model;
/// Bounded async concurrency pool.
pub mod pool;
/// Two-phase template filler.
pub mod template;
/// Pure function transforms (stub).
pub mod transform;

// Private: task YAML builder used by agent.rs.
pub(crate) mod task;

pub use agent::{AgentBuilder, AgentResult, AgentTask};
pub use auth::Auth;
pub use bundle::Bundle;
pub use command::{CommandBuilder, CommandResult};
pub use executor::Executor;
pub use model::{Model, Provider};
pub use pool::run_pool;
pub use template::TemplateFiller;
pub use transform::TransformBuilder;
