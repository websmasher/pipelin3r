//! Optional API key authentication middleware.
//!
//! When `SHEDUL3R_API_KEY` is set, all requests must include a valid
//! `Authorization: Bearer {key}` header. When unset, no authentication
//! is enforced (backward compatible for local development).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use actix_web::HttpResponse;
use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::StatusCode;
use actix_web::http::header;

/// Middleware factory that validates `Authorization: Bearer` tokens.
///
/// Created via [`ApiKeyAuth::new`] with the expected key. When applied
/// to a scope or app, it rejects requests without a valid token with
/// `401 Unauthorized` and a JSON error body.
#[derive(Clone, Debug)]
pub struct ApiKeyAuth {
    /// The expected API key value.
    key: Arc<String>,
}

impl ApiKeyAuth {
    /// Creates a new `ApiKeyAuth` middleware factory with the given expected key.
    pub fn new(key: String) -> Self {
        Self {
            key: Arc::new(key),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for ApiKeyAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Transform = ApiKeyAuthMiddleware<S>;
    type InitError = ();
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(ApiKeyAuthMiddleware {
            service: Arc::new(service),
            key: Arc::clone(&self.key),
        }))
    }
}

/// The actual middleware service that checks API keys on each request.
#[derive(Debug)]
pub struct ApiKeyAuthMiddleware<S> {
    /// The wrapped inner service.
    service: Arc<S>,
    /// The expected API key value.
    key: Arc<String>,
}

impl<S, B> Service<ServiceRequest> for ApiKeyAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &self,
        _ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let key = Arc::clone(&self.key);
        let service = Arc::clone(&self.service);

        Box::pin(async move {
            let auth_header = req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            let is_valid = auth_header.is_some_and(|h| {
                h.strip_prefix("Bearer ")
                    .is_some_and(|token| token == key.as_str())
            });

            if is_valid {
                let resp = service.call(req).await?;
                Ok(resp.map_into_left_body())
            } else {
                let body = serde_json::json!({
                    "error": "unauthorized",
                    "message": "missing or invalid API key"
                });
                let response = HttpResponse::build(StatusCode::UNAUTHORIZED)
                    .json(body);
                Ok(req.into_response(response).map_into_right_body())
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use actix_web::{App, test, web};

    use super::*;

    /// Dummy handler that returns 200 "ok".
    async fn ok_handler() -> &'static str {
        "ok"
    }

    #[actix_rt::test]
    async fn regression_request_without_auth_header_returns_401() {
        // Regression: there was no auth at all. When SHEDUL3R_API_KEY is
        // set, a request without an Authorization header must return 401.
        let app = test::init_service(
            App::new()
                .wrap(ApiKeyAuth::new("test-secret-key".to_owned()))
                .route("/test", web::get().to(ok_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/test").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "missing auth header must return 401"
        );

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("error").and_then(serde_json::Value::as_str),
            Some("unauthorized"),
        );
    }

    #[actix_rt::test]
    async fn regression_request_with_wrong_key_returns_401() {
        // Regression: auth must actually validate the key, not just check presence.
        let app = test::init_service(
            App::new()
                .wrap(ApiKeyAuth::new("correct-key".to_owned()))
                .route("/test", web::get().to(ok_handler)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", "Bearer wrong-key"))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "wrong key must return 401"
        );
    }

    #[actix_rt::test]
    async fn auth_passes_with_correct_key() {
        let app = test::init_service(
            App::new()
                .wrap(ApiKeyAuth::new("correct-key".to_owned()))
                .route("/test", web::get().to(ok_handler)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", "Bearer correct-key"))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "correct key must pass auth"
        );
    }
}
