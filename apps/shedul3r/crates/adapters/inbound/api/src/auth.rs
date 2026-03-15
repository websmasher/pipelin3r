//! Optional API key authentication middleware.
//!
//! When `SHEDUL3R_API_KEY` is set, all requests must include a valid
//! `Authorization: Bearer {key}` header. When unset, no authentication
//! is enforced (backward compatible for local development).

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Validate the `Authorization: Bearer` header against the expected key.
///
/// Intended to be used with [`axum::middleware::from_fn`] via a closure
/// that captures the expected key:
///
/// ```ignore
/// let key = "secret".to_owned();
/// app.layer(axum::middleware::from_fn(move |req, next| {
///     let k = key.clone();
///     auth::check_api_key(req, next, k)
/// }));
/// ```
///
/// Returns the downstream response on success, or `401 Unauthorized`
/// with a JSON error body on failure.
pub async fn check_api_key(request: Request, next: Next, expected_key: String) -> Response {
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let is_valid = auth_header.is_some_and(|header| {
        header
            .strip_prefix("Bearer ")
            .is_some_and(|token| token == expected_key)
    });

    if is_valid {
        next.run(request).await
    } else {
        let body = serde_json::json!({
            "error": "unauthorized",
            "message": "missing or invalid API key"
        });
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}
