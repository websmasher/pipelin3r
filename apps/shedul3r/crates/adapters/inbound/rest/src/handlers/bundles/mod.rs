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
        return Err(AppError::BadRequest("bundle path is empty".to_owned()));
    }
    let path = std::path::Path::new(name);
    for component in path.components() {
        match component {
            Component::Normal(_) => {} // OK — plain filename or directory name
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => {
                return Err(AppError::BadRequest(format!("invalid bundle path: {name}")));
            }
        }
    }
    Ok(())
}

/// Registers all bundle-related routes on the given service config.
pub fn configure_bundle_routes(cfg: &mut web::ServiceConfig) {
    #[allow(
        clippy::literal_string_with_formatting_args,
        reason = "actix-web route pattern contains {id}, not a format string"
    )]
    let _: &mut web::ServiceConfig = cfg
        .service(web::resource("/api/bundles").route(web::post().to(upload)))
        .service(web::resource("/api/bundles/{id}/files/{path:.*}").route(web::get().to(download)))
        .service(web::resource("/api/bundles/{id}").route(web::delete().to(delete_bundle)));
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
        let field =
            field_result.map_err(|e| AppError::BadRequest(format!("multipart read error: {e}")))?;

        file_count = file_count
            .checked_add(1)
            .ok_or_else(|| AppError::BadRequest("file count overflow".to_owned()))?;
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
            let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            file_size = file_size.saturating_add(chunk_len);
            total_size = total_size.saturating_add(chunk_len);
            if file_size > MAX_FIELD_SIZE {
                return Err(AppError::BadRequest("file exceeds 10MB limit".to_owned()));
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
mod tests;
