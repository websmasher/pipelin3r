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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::middleware;
    use axum::routing::get;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Dummy handler that returns 200 "ok".
    async fn ok_handler() -> &'static str {
        "ok"
    }

    /// Build a test app with auth middleware using the given key.
    fn app_with_auth(key: &str) -> axum::Router {
        let api_key = key.to_owned();
        axum::Router::new()
            .route("/test", get(ok_handler))
            .layer(middleware::from_fn(move |req, next| {
                let k = api_key.clone();
                check_api_key(req, next, k)
            }))
    }

    #[tokio::test]
    async fn regression_request_without_auth_header_returns_401() {
        // Regression: there was no auth at all. When SHEDUL3R_API_KEY is
        // set, a request without an Authorization header must return 401.
        let app = app_with_auth("test-secret-key");

        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "missing auth header must return 401"
        );

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("error").and_then(serde_json::Value::as_str),
            Some("unauthorized"),
        );
    }

    #[tokio::test]
    async fn regression_request_with_wrong_key_returns_401() {
        // Regression: auth must actually validate the key, not just check presence.
        let app = app_with_auth("correct-key");

        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/test")
            .header("Authorization", "Bearer wrong-key")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "wrong key must return 401"
        );
    }

    #[tokio::test]
    async fn auth_passes_with_correct_key() {
        let app = app_with_auth("correct-key");

        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/test")
            .header("Authorization", "Bearer correct-key")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "correct key must pass auth"
        );
    }
}
