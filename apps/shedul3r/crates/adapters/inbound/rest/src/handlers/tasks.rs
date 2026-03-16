//! Task execution HTTP handlers.
//!
//! Provides three endpoints:
//! - `POST /api/tasks` — execute a task
//! - `GET /api/tasks/status` — scheduler status
//! - `GET /api/tasks/limiter-status` — per-key limiter statuses

use std::sync::Arc;

use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::web;
use domain_types::SchedulrError;

use crate::state::AppState;

/// Registers all task-related routes on the given service config.
pub fn configure_task_routes(cfg: &mut web::ServiceConfig) {
    let _: &mut web::ServiceConfig = cfg
        .service(
            web::resource("/api/tasks").route(web::post().to(execute_task)),
        )
        .service(
            web::resource("/api/tasks/status").route(web::get().to(scheduler_status)),
        )
        .service(
            web::resource("/api/tasks/limiter-status").route(web::get().to(limiter_status)),
        );
}

/// Execute a task and return the result.
///
/// Returns HTTP 200 for successful execution (even if the subprocess fails),
/// or HTTP 400 if the task definition cannot be parsed.
async fn execute_task(
    state: web::Data<Arc<AppState>>,
    request: crate::ValidatedJson<domain_types::TaskRequest>,
) -> HttpResponse {
    let crate::ValidatedJson(request) = request;
    match state.engine.execute(request).await {
        Ok(response) => {
            let body = serde_json::to_value(&response).unwrap_or_default();
            HttpResponse::Ok().json(body)
        }
        Err(SchedulrError::TaskDefinition(msg)) => {
            let body = serde_json::json!({
                "error": "task_not_found",
                "message": msg,
            });
            HttpResponse::build(StatusCode::BAD_REQUEST).json(body)
        }
        Err(other) => {
            tracing::error!(error = %other, "task execution error");
            let body = serde_json::json!({
                "error": "internal_error",
                "message": "An internal error occurred",
            });
            HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(body)
        }
    }
}

/// Return the current scheduler status.
async fn scheduler_status(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let status = state.engine.status();
    HttpResponse::Ok().json(serde_json::to_value(&status).unwrap_or_default())
}

/// Return per-key limiter statuses.
async fn limiter_status(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let statuses = state.engine.limiter_status();
    HttpResponse::Ok().json(serde_json::to_value(&statuses).unwrap_or_default())
}
