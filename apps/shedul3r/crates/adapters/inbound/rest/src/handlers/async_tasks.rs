//! Async task submission and polling HTTP handlers.
//!
//! Provides two endpoints:
//! - `POST /api/tasks/async` -- submit a task for background execution (returns 202 + task ID)
//! - `GET /api/tasks/async/{id}` -- poll for the result of a previously submitted task

use std::sync::Arc;

use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::web;

use crate::state::AppState;

/// Registers async task routes on the given service config.
pub fn configure_async_task_routes(cfg: &mut web::ServiceConfig) {
    #[allow(
        clippy::literal_string_with_formatting_args,
        reason = "actix-web route pattern contains {id}, not a format string"
    )]
    let _: &mut web::ServiceConfig = cfg
        .service(web::resource("/api/tasks/async").route(web::post().to(submit_async)))
        .service(web::resource("/api/tasks/async/{id}").route(web::get().to(get_task_status)));
}

/// Submit a task for asynchronous execution.
///
/// Spawns a background tokio task that runs the task engine, then marks
/// the result as completed or failed in the async task store.
///
/// Returns HTTP 202 with `{ "task_id": "<uuid>" }`.
async fn submit_async(
    state: web::Data<Arc<AppState>>,
    request: crate::ValidatedJson<domain_types::TaskRequest>,
) -> HttpResponse {
    let crate::ValidatedJson(request) = request;
    let task_id = uuid::Uuid::new_v4().to_string();

    state.async_tasks.insert_running(task_id.clone());

    let engine = Arc::clone(&state.engine);
    let store = Arc::clone(&state.async_tasks);
    let id = task_id.clone();

    let _handle = tokio::spawn(async move {
        match engine.execute(request).await {
            Ok(response) => store.mark_completed(&id, response),
            Err(e) => store.mark_failed(&id, e.to_string()),
        }
    });

    let body = serde_json::json!({ "task_id": task_id });
    HttpResponse::build(StatusCode::ACCEPTED).json(body)
}

/// Poll the status of an async task by ID.
///
/// Returns:
/// - 200 with `{ "status": "running" }` if the task is still executing
/// - 200 with `{ "status": "completed", "result": { ... } }` if done
/// - 200 with `{ "status": "failed", "error": "..." }` if failed
/// - 404 if the task ID is unknown or has been reaped
async fn get_task_status(state: web::Data<Arc<AppState>>, id: web::Path<String>) -> HttpResponse {
    match state.async_tasks.get_status(&id) {
        Some(status) => HttpResponse::Ok().json(status),
        None => {
            let body = serde_json::json!({
                "error": "not_found",
                "message": format!("task not found: {id}"),
            });
            HttpResponse::build(StatusCode::NOT_FOUND).json(body)
        }
    }
}
