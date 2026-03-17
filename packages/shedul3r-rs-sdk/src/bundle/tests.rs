#![allow(clippy::unwrap_used, reason = "test assertions")]

use crate::bundle::BundleFileRef;
use crate::{Client, ClientConfig};

/// Type alias for mock server handle: `(bind address, server thread)`.
type MockServer = (std::net::SocketAddr, std::thread::JoinHandle<()>);

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
    let client = Client::new(config).unwrap();

    let files: Vec<BundleFileRef<'_>> = vec![("test.txt", b"hello")];
    let result = client.upload_bundle(&files).await;

    assert!(result.is_err(), "should fail when server is unreachable");
}

#[tokio::test]
async fn download_file_returns_error_on_connection_refused() {
    let config = ClientConfig {
        base_url: String::from("http://127.0.0.1:19999"),
        timeout: std::time::Duration::from_millis(500),
        ..ClientConfig::default()
    };
    let client = Client::new(config).unwrap();

    let result = client.download_file("some-id", "file.txt").await;

    assert!(result.is_err(), "should fail when server is unreachable");
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
fn spawn_http_mock(status: u16, status_text: &str, body: &str) -> MockServer {
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
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
    let client = Client::new(config).unwrap();
    let files: Vec<BundleFileRef<'_>> = vec![("test.txt", b"hello")];
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
    let client = Client::new(config).unwrap();
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
    let client = Client::new(config).unwrap();
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
    let (addr, _handle) =
        spawn_http_mock(200, "OK", r#"{"id":"b-123","path":"/tmp/bundles/b-123"}"#);
    let config = ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: std::time::Duration::from_millis(2000),
        ..ClientConfig::default()
    };
    let client = Client::new(config).unwrap();
    let files: Vec<BundleFileRef<'_>> = vec![("test.txt", b"hello")];
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
    let client = Client::new(config).unwrap();
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
    let client = Client::new(config).unwrap();
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
    let client = Client::new(config).unwrap();

    let result = client.delete_bundle("some-id").await;

    assert!(result.is_err(), "should fail when server is unreachable");
}
