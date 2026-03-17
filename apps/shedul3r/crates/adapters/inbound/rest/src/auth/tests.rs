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
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
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
