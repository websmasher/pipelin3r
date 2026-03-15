//! Bundle management HTTP handlers.
//!
//! Provides three endpoints:
//! - `POST /api/bundles` — upload a multipart file bundle
//! - `GET /api/bundles/:id/files/*path` — download a single file from a bundle
//! - `DELETE /api/bundles/:id` — delete a bundle and clean up its temp directory

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};

use crate::AppError;
use crate::state::AppState;

/// Builds an Axum router with all bundle-related endpoints.
pub fn bundle_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/bundles", post(upload))
        .route("/api/bundles/{id}/files/{*path}", get(download))
        .route("/api/bundles/{id}", delete(delete_bundle))
}

/// Accept a multipart upload, write files to a temp directory, return the bundle ID.
async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Internal(format!("failed to create temp dir: {e}")))?;

    let base = temp_dir.path().to_path_buf();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart read error: {e}")))?
    {
        let name = field
            .name()
            .ok_or_else(|| AppError::BadRequest("field missing name".to_owned()))?
            .to_owned();

        if name.is_empty() {
            return Err(AppError::BadRequest("field name is empty".to_owned()));
        }

        // Reject path traversal attempts.
        if name.contains("..") {
            return Err(AppError::BadRequest(
                "path traversal not allowed".to_owned(),
            ));
        }

        let file_path = base.join(&name);

        // Ensure parent directories exist.
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Internal(format!("failed to create dirs: {e}")))?;
        }

        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("failed to read field bytes: {e}")))?;

        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|e| AppError::Internal(format!("failed to write file: {e}")))?;
    }

    let bundle_id = uuid::Uuid::new_v4().to_string();
    let bundle_path = base.display().to_string();

    let entry = crate::state::BundleEntry::new(temp_dir);

    let _prev = state.bundles.write().insert(bundle_id.clone(), entry);

    let body = serde_json::json!({
        "id": bundle_id,
        "path": bundle_path,
    });

    Ok((StatusCode::OK, axum::Json(body)).into_response())
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
    State(state): State<Arc<AppState>>,
    Path(params): Path<DownloadParams>,
) -> Result<Response, AppError> {
    let file_path = params.path;
    let id = params.id;

    // Reject path traversal attempts.
    if file_path.contains("..") {
        return Err(AppError::BadRequest(
            "path traversal not allowed".to_owned(),
        ));
    }

    let bundle_base = state
        .bundles
        .read()
        .get(&id)
        .ok_or_else(|| AppError::NotFound(format!("bundle not found: {id}")))?
        .path
        .clone();

    let full_path = bundle_base.join(&file_path);

    let bytes = tokio::fs::read(&full_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!("file not found: {file_path}"))
        } else {
            AppError::Internal(format!("failed to read file: {e}"))
        }
    })?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Body::from(bytes),
    )
        .into_response())
}

/// Delete a bundle and clean up its temp directory.
async fn delete_bundle(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let removed = state.bundles.write().remove(&id);

    if removed.is_none() {
        return Err(AppError::NotFound(format!("bundle not found: {id}")));
    }

    // The BundleEntry is dropped here, which drops the TempDir and cleans up.
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::state::build_app_state;

    use super::bundle_router;

    /// Build a test app with just the bundle routes.
    fn test_app() -> axum::Router {
        let state = build_app_state();
        bundle_router().with_state(state)
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

    #[tokio::test]
    async fn upload_then_download() {
        let app = test_app();

        let file_content = b"hello world";
        let mp = multipart_body("test.txt", file_content);

        // Upload
        let upload_req = Request::builder()
            .method("POST")
            .uri("/api/bundles")
            .header("content-type", &mp.content_type)
            .body(Body::from(mp.body))
            .unwrap_or_default();

        let upload_resp = app
            .clone()
            .oneshot(upload_req)
            .await
            .unwrap_or_default();

        assert_eq!(upload_resp.status(), 200, "upload should succeed");

        let upload_body = upload_resp
            .into_body()
            .collect()
            .await
            .unwrap_or_default()
            .to_bytes();
        let upload_json: serde_json::Value =
            serde_json::from_slice(&upload_body).unwrap_or_default();

        let bundle_id = upload_json
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(!bundle_id.is_empty(), "bundle id should be non-empty");

        // Download
        let download_req = Request::builder()
            .method("GET")
            .uri(format!("/api/bundles/{bundle_id}/files/test.txt"))
            .body(Body::empty())
            .unwrap_or_default();

        let download_resp = app
            .oneshot(download_req)
            .await
            .unwrap_or_default();

        assert_eq!(download_resp.status(), 200, "download should succeed");

        let download_body = download_resp
            .into_body()
            .collect()
            .await
            .unwrap_or_default()
            .to_bytes();
        assert_eq!(
            download_body.as_ref(),
            file_content,
            "downloaded content should match uploaded content"
        );
    }

    #[tokio::test]
    async fn upload_then_delete() {
        let app = test_app();

        let mp = multipart_body("data.bin", b"binary data");

        // Upload
        let upload_req = Request::builder()
            .method("POST")
            .uri("/api/bundles")
            .header("content-type", &mp.content_type)
            .body(Body::from(mp.body))
            .unwrap_or_default();

        let upload_resp = app
            .clone()
            .oneshot(upload_req)
            .await
            .unwrap_or_default();

        assert_eq!(upload_resp.status(), 200, "upload should succeed");

        let upload_body = upload_resp
            .into_body()
            .collect()
            .await
            .unwrap_or_default()
            .to_bytes();
        let upload_json: serde_json::Value =
            serde_json::from_slice(&upload_body).unwrap_or_default();

        let bundle_id = upload_json
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        // Delete
        let delete_req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/bundles/{bundle_id}"))
            .body(Body::empty())
            .unwrap_or_default();

        let delete_resp = app
            .clone()
            .oneshot(delete_req)
            .await
            .unwrap_or_default();

        assert_eq!(
            delete_resp.status(),
            204,
            "delete should return 204 No Content"
        );

        // Verify bundle is gone — download should 404.
        let download_req = Request::builder()
            .method("GET")
            .uri(format!("/api/bundles/{bundle_id}/files/data.bin"))
            .body(Body::empty())
            .unwrap_or_default();

        let download_resp = app
            .oneshot(download_req)
            .await
            .unwrap_or_default();

        assert_eq!(
            download_resp.status(),
            404,
            "download after delete should return 404"
        );
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_404() {
        let app = test_app();

        let req = Request::builder()
            .method("DELETE")
            .uri("/api/bundles/nonexistent-id")
            .body(Body::empty())
            .unwrap_or_default();

        let resp = app.oneshot(req).await.unwrap_or_default();

        assert_eq!(resp.status(), 404, "deleting missing bundle should 404");
    }
}
