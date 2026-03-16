//! Bundle management HTTP handlers.
//!
//! Provides three endpoints:
//! - `POST /api/bundles` — upload a multipart file bundle
//! - `GET /api/bundles/:id/files/*path` — download a single file from a bundle
//! - `DELETE /api/bundles/:id` — delete a bundle and clean up its temp directory
//!
//! Known limitations:
//! - Orphaned `TempDir`s are not reaped if the server crashes before deletion.
//!   A TTL-based reaper should be added in a future iteration.

use std::path::Component;
use std::pin::Pin;
use std::sync::Arc;

use actix_multipart::Multipart;
use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::web;
use futures_core::Stream;

use crate::AppError;
use crate::state::AppState;

/// Maximum number of files allowed in a single bundle upload.
const MAX_FILES_PER_BUNDLE: usize = 100;

/// Maximum size of a single file field in bytes (10 MB).
const MAX_FIELD_SIZE: u64 = 10_000_000;

/// Maximum total size of all files in a single bundle upload (100 MB).
const MAX_TOTAL_BUNDLE_SIZE: u64 = 100_000_000;

/// Validate that a bundle path contains only normal components.
///
/// Rejects absolute paths (`/foo`), parent traversal (`../foo`), root (`/`),
/// and Windows prefix (`C:\`). Only `Component::Normal` segments are allowed.
fn validate_bundle_path(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::BadRequest(
            "bundle path is empty".to_owned(),
        ));
    }
    let path = std::path::Path::new(name);
    for component in path.components() {
        match component {
            Component::Normal(_) => {} // OK — plain filename or directory name
            Component::Prefix(_) | Component::RootDir | Component::CurDir | Component::ParentDir => {
                return Err(AppError::BadRequest(format!(
                    "invalid bundle path: {name}"
                )));
            }
        }
    }
    Ok(())
}

/// Registers all bundle-related routes on the given service config.
pub fn configure_bundle_routes(cfg: &mut web::ServiceConfig) {
    #[allow(clippy::literal_string_with_formatting_args)] // actix-web route pattern, not a format string
    let _: &mut web::ServiceConfig = cfg
        .service(
            web::resource("/api/bundles").route(web::post().to(upload)),
        )
        .service(
            web::resource("/api/bundles/{id}/files/{path:.*}").route(web::get().to(download)),
        )
        .service(
            web::resource("/api/bundles/{id}").route(web::delete().to(delete_bundle)),
        );
}

/// Poll the next item from a pinned stream.
async fn stream_next<S, T>(stream: &mut Pin<&mut S>) -> Option<T>
where
    S: Stream<Item = T>,
{
    std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await
}

/// Accept a multipart upload, write files to a temp directory, return the bundle ID.
#[allow(clippy::future_not_send)] // actix-web Multipart/Field are !Send by design (single-threaded workers)
async fn upload(
    state: web::Data<Arc<AppState>>,
    multipart: Multipart,
) -> Result<HttpResponse, AppError> {
    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Internal(format!("failed to create temp dir: {e}")))?;

    let base = temp_dir.path().to_path_buf();
    let mut file_count: usize = 0;
    let mut total_size: u64 = 0;

    let mut multipart = std::pin::pin!(multipart);

    while let Some(field_result) = stream_next(&mut multipart.as_mut()).await {
        let field = field_result
            .map_err(|e| AppError::BadRequest(format!("multipart read error: {e}")))?;

        file_count = file_count.checked_add(1).ok_or_else(|| {
            AppError::BadRequest("file count overflow".to_owned())
        })?;
        if file_count > MAX_FILES_PER_BUNDLE {
            return Err(AppError::BadRequest(format!(
                "too many files: maximum {MAX_FILES_PER_BUNDLE} per bundle"
            )));
        }

        let name = field
            .name()
            .ok_or_else(|| AppError::BadRequest("field missing name".to_owned()))?
            .to_owned();

        // Validate the path: only normal components allowed (no `..`, `/`, `\`, etc.).
        validate_bundle_path(&name)?;

        let file_path = base.join(&name);

        // Ensure parent directories exist.
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Internal(format!("failed to create dirs: {e}")))?;
        }

        // Stream the field to disk, checking size limits incrementally.
        let mut file = tokio::fs::File::create(&file_path)
            .await
            .map_err(|e| AppError::Internal(format!("failed to create file: {e}")))?;
        let mut file_size: u64 = 0;

        let mut field_pinned = std::pin::pin!(field);
        while let Some(chunk_result) = stream_next(&mut field_pinned.as_mut()).await {
            let chunk = chunk_result
                .map_err(|e| AppError::BadRequest(format!("failed to read field chunk: {e}")))?;
            let chunk_len =
                u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            file_size = file_size.saturating_add(chunk_len);
            total_size = total_size.saturating_add(chunk_len);
            if file_size > MAX_FIELD_SIZE {
                return Err(AppError::BadRequest(
                    "file exceeds 10MB limit".to_owned(),
                ));
            }
            if total_size > MAX_TOTAL_BUNDLE_SIZE {
                return Err(AppError::BadRequest(
                    "bundle exceeds 100MB total limit".to_owned(),
                ));
            }
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                .await
                .map_err(|e| AppError::Internal(format!("failed to write file: {e}")))?;
        }
    }

    let bundle_id = uuid::Uuid::new_v4().to_string();
    // NOTE: The path is intentionally included in the response. The SDK's
    // `BundleHandle` uses it as `working_directory` when submitting tasks.
    // It leaks the temp dir structure but not sensitive data.
    let bundle_path = base.display().to_string();

    let entry = crate::state::BundleEntry::new(temp_dir);

    let _prev = state.bundles.write().insert(bundle_id.clone(), entry);

    let body = serde_json::json!({
        "id": bundle_id,
        "path": bundle_path,
    });

    Ok(HttpResponse::Ok().json(body))
}

/// Path parameters for the file download endpoint.
#[derive(serde::Deserialize)]
struct DownloadParams {
    /// Bundle identifier.
    id: String,
    /// Relative file path within the bundle.
    path: String,
}

/// Download a single file from a bundle by its relative path.
async fn download(
    state: web::Data<Arc<AppState>>,
    params: web::Path<DownloadParams>,
) -> Result<HttpResponse, AppError> {
    let file_path = &params.path;
    let id = &params.id;

    // Validate the path: only normal components allowed (no `..`, `/`, `\`, etc.).
    validate_bundle_path(file_path)?;

    let bundle_base = state
        .bundles
        .read()
        .get(id)
        .ok_or_else(|| AppError::NotFound(format!("bundle not found: {id}")))?
        .path
        .clone();

    let full_path = bundle_base.join(file_path);

    let content = tokio::fs::read(&full_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!("file not found: {file_path}"))
        } else {
            AppError::Internal(format!("failed to open file: {e}"))
        }
    })?;

    Ok(HttpResponse::Ok()
        .content_type("application/octet-stream")
        .body(content))
}

/// Delete a bundle and clean up its temp directory.
async fn delete_bundle(
    state: web::Data<Arc<AppState>>,
    id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let removed = state.bundles.write().remove(id.as_str());

    if removed.is_none() {
        return Err(AppError::NotFound(format!("bundle not found: {id}")));
    }

    // The BundleEntry is dropped here, which drops the TempDir and cleans up.
    Ok(HttpResponse::build(StatusCode::NO_CONTENT).finish())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
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
    fn multipart_body(
        field_name: &str,
        content: &[u8],
    ) -> MultipartRequest {
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

        let content_type =
            format!("multipart/form-data; boundary={boundary}");
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
        let upload_json: serde_json::Value =
            serde_json::from_slice(&upload_body).unwrap_or_default();

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
        let upload_json: serde_json::Value =
            serde_json::from_slice(&upload_body).unwrap_or_default();

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
        let upload_json: serde_json::Value =
            serde_json::from_slice(&upload_body).unwrap_or_default();
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
        assert_eq!(
            resp.status(),
            400,
            "upload of >10MB field must return 400"
        );
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
}
