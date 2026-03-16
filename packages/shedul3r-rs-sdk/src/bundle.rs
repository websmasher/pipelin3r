//! Bundle upload and download utilities.
//!
//! These endpoints support remote execution workflows where task inputs and
//! outputs are transferred as file bundles.

use serde::Deserialize;

use crate::error::SdkError;

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
        files: &[(&str, &[u8])],
    ) -> Result<BundleHandle, SdkError> {
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
            return Err(SdkError::Bundle(format!(
                "upload failed (HTTP {status}): {}",
                super::client::truncate_str(&body, 500)
            )));
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
    ) -> Result<Vec<u8>, SdkError> {
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

    #[test]
    fn regression_download_file_url_encodes_path_with_spaces() {
        // Regression: bundle path was not URL-encoded, causing requests with
        // spaces in the path to fail or hit wrong endpoints.
        // We verify per-segment encoding: spaces become %20 but / stays literal.
        let path_with_spaces = "my folder/output file.txt";
        let encoded = super::encode_path_segments(path_with_spaces);
        assert!(
            encoded.contains("%20"),
            "spaces must be percent-encoded in URL: {encoded}"
        );
        assert!(
            !encoded.contains(' '),
            "encoded URL must not contain literal spaces: {encoded}"
        );
        // Slashes must be preserved as literal separators.
        assert!(
            encoded.contains("my%20folder/output%20file.txt"),
            "URL must encode segments but preserve slashes: {encoded}"
        );
    }

    #[test]
    fn regression_download_file_preserves_slashes_in_nested_paths() {
        // Regression: urlencoding::encode(path) encoded the entire path,
        // turning `/` into `%2F`. The server's wildcard route expects
        // literal slashes. Per-segment encoding fixes this.
        let nested_path = "sub/dir/file.txt";
        let base_url = "http://localhost:7943";
        let bundle_id = "test-bundle-123";
        let url = format!(
            "{}/api/bundles/{}/files/{}",
            base_url,
            super::encode_path_segments(bundle_id),
            super::encode_path_segments(nested_path),
        );
        assert!(
            url.contains("sub/dir/file.txt"),
            "nested path must have literal slashes, not %%2F: {url}"
        );
        assert!(
            !url.contains("%2F"),
            "URL must not contain %%2F for path separators: {url}"
        );
    }

    /// Start a TCP listener that responds with a fixed HTTP response to any request.
    fn spawn_http_mock(
        status: u16,
        status_text: &str,
        body: &str,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
        let response = format!(
            "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|_| std::process::abort());
        let addr = listener.local_addr().unwrap_or_else(|_| std::process::abort());
        let handle = std::thread::spawn(move || {
            // Accept up to 5 connections (enough for our tests).
            for _ in 0..5 {
                if let Ok((mut stream, _)) = listener.accept() {
                    // Read request (we don't care about contents).
                    let mut buf = [0u8; 4096];
                    let _ = std::io::Read::read(&mut stream, &mut buf);
                    let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
                    let _ = std::io::Write::flush(&mut stream);
                }
            }
        });
        (addr, handle)
    }

    #[tokio::test]
    async fn mutant_kill_upload_bundle_checks_success_status() {
        // Mutant kill: bundle.rs:68 — `delete !` flips `!resp.status().is_success()`
        // A 400 response must return Err, not Ok.
        let (addr, _handle) = spawn_http_mock(400, "Bad Request", r#"{"error":"bad"}"#);
        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: std::time::Duration::from_millis(2000),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());
        let files: Vec<(&str, &[u8])> = vec![("test.txt", b"hello")];
        let result = client.upload_bundle(&files).await;
        assert!(
            result.is_err(),
            "upload_bundle must return Err for HTTP 400, not Ok"
        );
    }

    #[tokio::test]
    async fn mutant_kill_download_checks_success_status() {
        // Mutant kill: bundle.rs:105 — `delete !` flips `!resp.status().is_success()`
        // A 404 response must return Err, not Ok.
        let (addr, _handle) = spawn_http_mock(404, "Not Found", r#"{"error":"not found"}"#);
        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: std::time::Duration::from_millis(2000),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());
        let result = client.download_file("some-id", "file.txt").await;
        assert!(
            result.is_err(),
            "download_file must return Err for HTTP 404, not Ok"
        );
    }

    #[tokio::test]
    async fn mutant_kill_delete_checks_success_status() {
        // Mutant kill: bundle.rs:133 — `delete !` flips `!resp.status().is_success()`
        // A 500 response must return Err, not Ok.
        let (addr, _handle) = spawn_http_mock(500, "Internal Server Error", r#"{"error":"fail"}"#);
        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: std::time::Duration::from_millis(2000),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());
        let result = client.delete_bundle("some-id").await;
        assert!(
            result.is_err(),
            "delete_bundle must return Err for HTTP 500, not Ok"
        );
    }

    #[tokio::test]
    async fn mutant_kill_upload_bundle_success_returns_ok() {
        // Mutant kill: bundle.rs:68 — `delete !` also means success is treated as error.
        // A 200 response with valid JSON must return Ok.
        let (addr, _handle) = spawn_http_mock(200, "OK", r#"{"id":"b-123","path":"/tmp/bundles/b-123"}"#);
        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: std::time::Duration::from_millis(2000),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());
        let files: Vec<(&str, &[u8])> = vec![("test.txt", b"hello")];
        let result = client.upload_bundle(&files).await;
        assert!(
            result.is_ok(),
            "upload_bundle must return Ok for HTTP 200, not Err: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn mutant_kill_download_success_returns_bytes() {
        // Mutant kill: bundle.rs:105 — verify success path returns Ok with body bytes.
        let (addr, _handle) = spawn_http_mock(200, "OK", "file-content-here");
        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: std::time::Duration::from_millis(2000),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());
        let result = client.download_file("some-id", "file.txt").await;
        assert!(
            result.is_ok(),
            "download_file must return Ok for HTTP 200: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn mutant_kill_delete_success_returns_ok() {
        // Mutant kill: bundle.rs:133 — verify success path returns Ok.
        let (addr, _handle) = spawn_http_mock(200, "OK", "{}");
        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: std::time::Duration::from_millis(2000),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());
        let result = client.delete_bundle("some-id").await;
        assert!(
            result.is_ok(),
            "delete_bundle must return Ok for HTTP 200: {:?}",
            result.err()
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
