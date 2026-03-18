//! Domain types: entities, value objects, and domain errors for the schedulr engine.

use std::collections::BTreeMap;

pub mod async_task;
pub mod config;
pub mod duration_serde;
pub mod error;
pub mod request;
pub mod subprocess;
pub mod task;

pub use async_task::{AsyncTaskState, AsyncTaskStatus};
pub use config::{BulkheadConfig, CircuitBreakerConfig, RateLimitConfig, RetryConfig};
pub use error::SchedulrError;
pub use request::{
    ExecutionMetadata, LimiterKeyStatus, SchedulerStatus, TaskRequest, TaskResponse,
};
pub use subprocess::{SubprocessCommand, SubprocessResult};
pub use task::TaskDefinition;

/// Environment variables map: key-value pairs passed to subprocesses.
pub type EnvironmentMap = BTreeMap<String, String>;
