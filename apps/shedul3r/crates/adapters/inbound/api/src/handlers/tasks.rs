//! Task execution HTTP handlers.
//!
//! Provides three endpoints:
//! - `POST /api/tasks` — execute a task
//! - `GET /api/tasks/status` — scheduler status
//! - `GET /api/tasks/limiter-status` — per-key limiter statuses

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use domain_types::SchedulrError;

use crate::state::AppState;

/// Builds an Axum router with all task-related endpoints.
pub fn task_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tasks", post(execute_task))
        .route("/api/tasks/status", get(scheduler_status))
        .route("/api/tasks/limiter-status", get(limiter_status))
}

/// Execute a task and return the result.
///
/// Returns HTTP 200 for successful execution (even if the subprocess fails),
/// or HTTP 400 if the task definition cannot be parsed.
async fn execute_task(
    State(state): State<Arc<AppState>>,
    crate::ValidatedJson(request): crate::ValidatedJson<domain_types::TaskRequest>,
) -> Response {
    match state.engine.execute(request).await {
        Ok(response) => {
            let body = serde_json::to_value(&response).unwrap_or_default();
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(SchedulrError::TaskDefinition(msg)) => {
            let body = serde_json::json!({
                "error": "task_not_found",
                "message": msg,
            });
            (StatusCode::BAD_REQUEST, Json(body)).into_response()
        }
        Err(other) => {
            tracing::error!(error = %other, "task execution error");
            let body = serde_json::json!({
                "error": "internal_error",
                "message": "An internal error occurred",
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
        }
    }
}

/// Return the current scheduler status.
async fn scheduler_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let status = state.engine.status();
    Json(serde_json::to_value(&status).unwrap_or_default())
}

/// Return per-key limiter statuses.
async fn limiter_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let statuses = state.engine.limiter_status();
    Json(serde_json::to_value(&statuses).unwrap_or_default())
}
