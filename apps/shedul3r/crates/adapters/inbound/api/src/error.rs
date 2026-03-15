use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Classified error enum mapping all application errors to HTTP status codes.
///
/// Each variant corresponds to a specific HTTP response category. Domain-specific
/// errors are converted via `From` impls added per entity (see scaffold output).
///
/// Selective logging rule: Internal and External errors are logged server-side.
/// `BadRequest` errors are NOT logged — client mistakes are not server concerns.
#[derive(Debug)]
pub enum AppError {
    /// Client sent invalid input — 400
    BadRequest(String),
    /// Resource not found — 404
    NotFound(String),
    /// Server-side failure — 500
    Internal(String),
    /// Upstream service failure — 502
    External(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_owned(),
                )
            }
            Self::External(msg) => {
                tracing::error!(error = %msg, "external service error");
                (
                    StatusCode::BAD_GATEWAY,
                    "external_error",
                    "An external service error occurred".to_owned(),
                )
            }
        };
        let body = serde_json::json!({ "error": code, "message": message });
        (status, Json(body)).into_response()
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::BadRequest(e.to_string())
    }
}
