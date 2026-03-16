use actix_web::HttpResponse;
use actix_web::http::StatusCode;

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

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "bad request: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
            Self::External(msg) => write!(f, "external error: {msg}"),
        }
    }
}

impl actix_web::error::ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::External(_) => StatusCode::BAD_GATEWAY,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let (code, message) = match self {
            Self::BadRequest(msg) => ("bad_request", msg.clone()),
            Self::NotFound(msg) => ("not_found", msg.clone()),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal error");
                (
                    "internal_error",
                    "An internal error occurred".to_owned(),
                )
            }
            Self::External(msg) => {
                tracing::error!(error = %msg, "external service error");
                (
                    "external_error",
                    "An external service error occurred".to_owned(),
                )
            }
        };
        let body = serde_json::json!({ "error": code, "message": message });
        HttpResponse::build(self.status_code()).json(body)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::BadRequest(e.to_string())
    }
}
