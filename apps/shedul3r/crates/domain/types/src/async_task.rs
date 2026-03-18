//! Types for async task submission and polling.
//!
//! Async tasks are submitted via `POST /api/tasks/async` and polled via
//! `GET /api/tasks/async/{id}`. These types represent the state machine
//! and the client-facing status response.

use serde::{Deserialize, Serialize};

use crate::TaskResponse;

/// Internal state of an async task as it moves through its lifecycle.
#[derive(Debug, Clone)]
pub enum AsyncTaskState {
    /// Task is currently executing in a background tokio task.
    Running,
    /// Task completed (successfully or with a non-zero exit code).
    Completed(TaskResponse),
    /// Task failed with an infrastructure error (not a subprocess failure).
    Failed(String),
}

/// Status response returned to the client when polling an async task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncTaskStatus {
    /// Current state: `"running"`, `"completed"`, or `"failed"`.
    pub status: String,
    /// Task result, present only when status is `"completed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResponse>,
    /// Error message, present only when status is `"failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
