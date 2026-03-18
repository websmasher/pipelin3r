use actix_web::{App, test, web};

use crate::state::build_app_state;

use super::configure_bundle_routes;

macro_rules! test_app {
    () => {{
        let state = build_app_state();
        test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(configure_bundle_routes),
        )
        .await
    }};
}

/// Multipart request: content-type header + body bytes.
struct MultipartRequest {
    content_type: String,
    body: Vec<u8>,
}

/// Build a multipart body with a single file part.
fn multipart_body(field_name: &str, content: &[u8]) -> MultipartRequest {
    let boundary = "----TestBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{field_name}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let content_type = format!("multipart/form-data; boundary={boundary}");
    MultipartRequest { content_type, body }
}

#[actix_rt::test]
async fn upload_then_download() {
    let app = test_app!();

    let file_content = b"hello world";
    let mp = multipart_body("test.txt", file_content);

    // Upload
    let upload_req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let upload_resp = test::call_service(&app, upload_req).await;
    assert_eq!(upload_resp.status(), 200, "upload should succeed");

    let upload_body = test::read_body(upload_resp).await;
    let upload_json: serde_json::Value = serde_json::from_slice(&upload_body).unwrap_or_default();

    let bundle_id = upload_json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(!bundle_id.is_empty(), "bundle id should be non-empty");

    // Download
    let download_req = test::TestRequest::get()
        .uri(&format!("/api/bundles/{bundle_id}/files/test.txt"))
        .to_request();

    let download_resp = test::call_service(&app, download_req).await;
    assert_eq!(download_resp.status(), 200, "download should succeed");

    let download_body = test::read_body(download_resp).await;
    assert_eq!(
        download_body.as_ref(),
        file_content,
        "downloaded content should match uploaded content"
    );
}

#[actix_rt::test]
async fn upload_then_delete() {
    let app = test_app!();

    let mp = multipart_body("data.bin", b"binary data");

    // Upload
    let upload_req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let upload_resp = test::call_service(&app, upload_req).await;
    assert_eq!(upload_resp.status(), 200, "upload should succeed");

    let upload_body = test::read_body(upload_resp).await;
    let upload_json: serde_json::Value = serde_json::from_slice(&upload_body).unwrap_or_default();

    let bundle_id = upload_json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    // Delete
    let delete_req = test::TestRequest::delete()
        .uri(&format!("/api/bundles/{bundle_id}"))
        .to_request();

    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(
        delete_resp.status(),
        204,
        "delete should return 204 No Content"
    );

    // Verify bundle is gone — download should 404.
    let download_req = test::TestRequest::get()
        .uri(&format!("/api/bundles/{bundle_id}/files/data.bin"))
        .to_request();

    let download_resp = test::call_service(&app, download_req).await;
    assert_eq!(
        download_resp.status(),
        404,
        "download after delete should return 404"
    );
}

#[actix_rt::test]
async fn upload_rejects_absolute_path() {
    let app = test_app!();

    let mp = multipart_body("/etc/passwd", b"pwned");

    let req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "absolute path should be rejected");
}

#[actix_rt::test]
async fn upload_rejects_parent_traversal() {
    let app = test_app!();

    let mp = multipart_body("../etc/passwd", b"pwned");

    let req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "parent traversal should be rejected");
}

#[actix_rt::test]
async fn upload_rejects_mixed_traversal() {
    let app = test_app!();

    let mp = multipart_body("foo/../bar", b"pwned");

    let req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "mixed traversal should be rejected");
}

#[actix_rt::test]
async fn upload_accepts_nested_path() {
    let app = test_app!();

    let mp = multipart_body("src/lib.rs", b"fn main() {}");

    let req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "nested normal path should be accepted");
}

#[actix_rt::test]
async fn download_rejects_traversal() {
    let app = test_app!();

    // First upload a valid file so the bundle exists.
    let mp = multipart_body("test.txt", b"hello");

    let upload_req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let upload_resp = test::call_service(&app, upload_req).await;

    let upload_body = test::read_body(upload_resp).await;
    let upload_json: serde_json::Value = serde_json::from_slice(&upload_body).unwrap_or_default();
    let bundle_id = upload_json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    // Attempt to download with path traversal.
    let download_req = test::TestRequest::get()
        .uri(&format!(
            "/api/bundles/{bundle_id}/files/../../../etc/passwd"
        ))
        .to_request();

    let download_resp = test::call_service(&app, download_req).await;
    assert_eq!(
        download_resp.status(),
        400,
        "download with traversal should be rejected"
    );
}

#[actix_rt::test]
async fn regression_upload_rejects_oversized_field() {
    // Regression: there was no body size limit. A field exceeding 10MB
    // must return 400.
    let app = test_app!();

    // Create a field just over 10MB
    let content = vec![0u8; 10_000_001];
    let mp = multipart_body("big.bin", &content);

    let req = test::TestRequest::post()
        .uri("/api/bundles")
        .insert_header(("content-type", mp.content_type.as_str()))
        .set_payload(mp.body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "upload of >10MB field must return 400");
}

#[actix_rt::test]
async fn delete_nonexistent_returns_404() {
    let app = test_app!();

    let req = test::TestRequest::delete()
        .uri("/api/bundles/nonexistent-id")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "deleting missing bundle should 404");
}
