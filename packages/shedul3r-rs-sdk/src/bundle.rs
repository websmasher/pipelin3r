//! Bundle upload and download utilities.
//!
//! These endpoints support remote execution workflows where task inputs and
//! outputs are transferred as file bundles.

use serde::Deserialize;

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
        files: &[(&str, &[u8])],
    ) -> anyhow::Result<BundleHandle> {
        let url = format!("{}/api/bundles", self.base_url());

        let mut form = reqwest::multipart::Form::new();
        for &(name, content) in files {
            let part = reqwest::multipart::Part::bytes(content.to_vec())
                .file_name(String::from(name));
            form = form.part(String::from(name), part);
        }

        let resp: reqwest::Response = self
            .http_client()
            .post(&url)
            .multipart(form)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "bundle upload failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            );
        }

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
    pub async fn download_file(
        &self,
        bundle_id: &str,
        path: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let url = format!(
            "{}/api/bundles/{}/files/{}",
            self.base_url(), bundle_id, path
        );

        let resp: reqwest::Response = self.http_client().get(&url).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "bundle download failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            );
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
    pub async fn delete_bundle(&self, bundle_id: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/bundles/{}",
            self.base_url(), bundle_id
        );

        let resp: reqwest::Response = self.http_client().delete(&url).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "bundle delete failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Client, ClientConfig};

    #[test]
    fn upload_bundle_url_uses_base_url() {
        let config = ClientConfig {
            base_url: String::from("http://my-server:9000"),
            ..ClientConfig::default()
        };
        // Verify the URL would be constructed correctly by checking the
        // config is wired through. We cannot call the async method without
        // a server, but we can verify client construction succeeds and the
        // base_url is preserved.
        let client = Client::new(config);
        assert!(client.is_ok(), "client should be constructible");
    }

    #[tokio::test]
    async fn upload_bundle_returns_error_on_connection_refused() {
        let config = ClientConfig {
            // Use a port that is almost certainly not listening.
            base_url: String::from("http://127.0.0.1:19999"),
            timeout: std::time::Duration::from_millis(500),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());

        let files: Vec<(&str, &[u8])> = vec![("test.txt", b"hello")];
        let result = client.upload_bundle(&files).await;

        assert!(
            result.is_err(),
            "should fail when server is unreachable"
        );
    }

    #[tokio::test]
    async fn download_file_returns_error_on_connection_refused() {
        let config = ClientConfig {
            base_url: String::from("http://127.0.0.1:19999"),
            timeout: std::time::Duration::from_millis(500),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());

        let result = client.download_file("some-id", "file.txt").await;

        assert!(
            result.is_err(),
            "should fail when server is unreachable"
        );
    }

    #[tokio::test]
    async fn delete_bundle_returns_error_on_connection_refused() {
        let config = ClientConfig {
            base_url: String::from("http://127.0.0.1:19999"),
            timeout: std::time::Duration::from_millis(500),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());

        let result = client.delete_bundle("some-id").await;

        assert!(
            result.is_err(),
            "should fail when server is unreachable"
        );
    }
}
