//! Bundle upload and download utilities.
//!
//! These endpoints support remote execution workflows where task inputs and
//! outputs are transferred as file bundles.

use serde::Deserialize;

use crate::error::SdkError;

/// A file entry for bundle upload: `(name, content)`.
pub type BundleFileRef<'a> = (&'a str, &'a [u8]);

#[cfg(test)]
mod tests;

/// URL-encode each segment of a path individually, preserving `/` separators.
///
/// Encoding the entire path would turn `/` into `%2F`, breaking server-side
/// wildcard route matching that expects literal slashes.
fn encode_path_segments(path: &str) -> String {
    path.split('/')
        .map(|segment| urlencoding::encode(segment))
        .collect::<Vec<_>>()
        .join("/")
}

/// Opaque handle returned after uploading a bundle.
#[derive(Debug, Clone)]
pub struct BundleHandle {
    /// Server-assigned identifier for the uploaded bundle.
    pub id: String,
    /// Remote path where the bundle is stored.
    pub remote_path: String,
}

/// JSON response from `POST /api/bundles`.
#[derive(Deserialize)]
struct UploadResponse {
    id: String,
    path: String,
}

impl super::Client {
    /// Upload a bundle of files. Returns a handle with the remote path.
    ///
    /// Each file is sent as a multipart form field where the field name is the
    /// relative path and the value is the file content.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response cannot be
    /// parsed.
    pub async fn upload_bundle(
        &self,
        files: &[BundleFileRef<'_>],
    ) -> Result<BundleHandle, SdkError> {
        let url = format!("{}/api/bundles", self.base_url());

        let mut form = reqwest::multipart::Form::new();
        for (idx, &(name, content)) in files.iter().enumerate() {
            let part =
                reqwest::multipart::Part::bytes(content.to_vec()).file_name(String::from(name));
            // Use numeric index as part name — actix multipart rejects slashes
            // in part names. The actual path is in Content-Disposition filename,
            // which the server reads via content_disposition().get_filename().
            form = form.part(format!("file{idx}"), part);
        }

        let resp: reqwest::Response = self.http_client().post(&url).multipart(form).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Bundle(format!(
                "upload failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            )));
        }

        #[allow(
            clippy::disallowed_methods,
            reason = "SDK client: thin HTTP wrapper, validation is caller's responsibility"
        )]
        let parsed: UploadResponse = resp.json().await?;

        Ok(BundleHandle {
            id: parsed.id,
            remote_path: parsed.path,
        })
    }

    /// Download a file from a previously uploaded bundle.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the server returns a
    /// non-success status.
    pub async fn download_file(&self, bundle_id: &str, path: &str) -> Result<Vec<u8>, SdkError> {
        let url = format!(
            "{}/api/bundles/{}/files/{}",
            self.base_url(),
            encode_path_segments(bundle_id),
            encode_path_segments(path),
        );

        let resp: reqwest::Response = self.http_client().get(&url).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Bundle(format!(
                "download failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            )));
        }

        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Delete a previously uploaded bundle.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the server returns a
    /// non-success status.
    pub async fn delete_bundle(&self, bundle_id: &str) -> Result<(), SdkError> {
        let url = format!(
            "{}/api/bundles/{}",
            self.base_url(),
            encode_path_segments(bundle_id),
        );

        let resp: reqwest::Response = self.http_client().delete(&url).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Bundle(format!(
                "delete failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            )));
        }

        Ok(())
    }
}
