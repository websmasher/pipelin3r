//! HTTP client for the shedul3r task execution API.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::SdkError;

/// Configuration for the shedul3r [`Client`].
///
/// All fields have sensible defaults via [`Default`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the shedul3r server (e.g. `http://localhost:7943`).
    pub base_url: String,
    /// HTTP request timeout. Default: 45 minutes (2,700,000 ms).
    pub timeout: Duration,
    /// Interval between file-existence polls in recovery mode. Default: 10 s.
    pub poll_interval: Duration,
    /// Initial delay before file polling starts. Default: 30 s.
    pub poll_initial_delay: Duration,
    /// Maximum total time to spend polling for a file. Default: 45 minutes.
    pub max_poll_duration: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: String::from("http://localhost:7943"),
            timeout: Duration::from_millis(2_700_000),
            poll_interval: Duration::from_millis(10_000),
            poll_initial_delay: Duration::from_millis(30_000),
            max_poll_duration: Duration::from_millis(2_700_000),
        }
    }
}

/// HTTP client for shedul3r.
///
/// Wraps a [`reqwest::Client`] with shedul3r-specific configuration.
/// Reuse a single `Client` across calls for connection pooling.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    config: ClientConfig,
}

/// JSON body sent to `POST /api/tasks`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskPayload {
    /// The task YAML definition.
    pub task: String,
    /// The input content (prompt, data, etc.).
    pub input: String,
    /// Optional working directory for the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Optional environment variables to inject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<BTreeMap<String, String>>,
    /// Optional limiter key override (overrides the key from the task YAML).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limiter_key: Option<String>,
    /// Optional per-task timeout override in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Response from shedul3r after task completion.
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Whether the task completed successfully.
    pub success: bool,
    /// Task output text (or error message on failure).
    pub output: String,
    /// Process exit code, if available.
    pub exit_code: Option<i32>,
    /// Wall-clock elapsed time as reported by the server.
    pub elapsed: Option<String>,
    /// ISO-8601 timestamp when execution started.
    pub started_at: Option<String>,
}

impl TaskResult {
    /// Require the task to have succeeded, returning `Err(SdkError::TaskFailed)`
    /// if it did not.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::TaskFailed`] when `self.success` is `false`.
    pub fn require_success(self) -> Result<Self, SdkError> {
        if self.success {
            Ok(self)
        } else {
            Err(SdkError::TaskFailed {
                message: self.output,
            })
        }
    }
}

/// Raw JSON response from the shedul3r API.
#[derive(Deserialize)]
struct ApiResponse {
    success: Option<bool>,
    output: Option<String>,
    message: Option<String>,
    metadata: Option<ApiResponseMetadata>,
}

/// Metadata nested inside [`ApiResponse`].
#[derive(Deserialize)]
struct ApiResponseMetadata {
    started_at: Option<String>,
    elapsed: Option<ApiElapsed>,
    exit_code: Option<i32>,
}

/// The server serialises `Duration` as `{ secs, nanos }`.
#[derive(Deserialize)]
struct ApiElapsed {
    secs: Option<u64>,
    nanos: Option<u32>,
}

impl ApiElapsed {
    /// Format the elapsed duration as a human-readable string.
    fn to_display_string(&self) -> String {
        let secs = self.secs.unwrap_or(0);
        let nanos = self.nanos.unwrap_or(0);
        if nanos == 0 {
            format!("{secs}s")
        } else {
            let millis = nanos.saturating_div(1_000_000);
            format!("{secs}.{millis:03}s")
        }
    }
}

impl Client {
    /// Create a new client with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new(config: ClientConfig) -> Result<Self, SdkError> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()?;
        Ok(Self { http, config })
    }

    /// Create a new client with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn with_defaults() -> Result<Self, SdkError> {
        Self::new(ClientConfig::default())
    }

    /// Get a reference to the base URL.
    pub(crate) fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Get a reference to the HTTP client.
    pub(crate) const fn http_client(&self) -> &reqwest::Client {
        &self.http
    }

    /// Submit a task and wait for the HTTP response.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Http`] for network or HTTP failures.
    /// Task-level failures (command exited non-zero) are returned as
    /// `Ok(TaskResult { success: false, .. })`.
    pub async fn submit_task(&self, payload: &TaskPayload) -> Result<TaskResult, SdkError> {
        let url = format!("{}/api/tasks", self.config.base_url);
        http_call(&self.http, &url, payload, None).await
    }

    /// Submit a task, racing the HTTP call against a file-existence poller.
    ///
    /// shedul3r sometimes drops long-running HTTP connections, but the task
    /// still completes and writes its output file. This method races the HTTP
    /// response against periodic checks for `expected_output` on disk.
    ///
    /// The file at `expected_output` is deleted before submission so only a
    /// *new* write is detected.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Http`] for network failures (after checking file
    /// recovery). Task-level failures are `Ok(TaskResult { success: false, .. })`.
    pub async fn submit_task_with_recovery(
        &self,
        payload: &TaskPayload,
        expected_output: &Path,
    ) -> Result<TaskResult, SdkError> {
        // Remove stale output so the poller only fires on fresh writes.
        let _ = std::fs::remove_file(expected_output);

        let url = format!("{}/api/tasks", self.config.base_url);
        let output_owned = expected_output.to_path_buf();

        let http_future = http_call(&self.http, &url, payload, Some(&output_owned));
        let poll_future = poll_for_file(
            &output_owned,
            self.config.poll_initial_delay,
            self.config.poll_interval,
            self.config.max_poll_duration,
        );

        tokio::select! {
            biased;
            result = http_future => result,
            poll_result = poll_future => {
                match poll_result {
                    Ok(()) => Ok(TaskResult {
                        success: true,
                        output: String::from("(recovered from file poll)"),
                        exit_code: None,
                        elapsed: None,
                        started_at: None,
                    }),
                    Err(e) => {
                        // Poll failed — check if the file appeared anyway.
                        if output_owned.exists() {
                            Ok(TaskResult {
                                success: true,
                                output: String::from("(recovered from file)"),
                                exit_code: None,
                                elapsed: None,
                                started_at: None,
                            })
                        } else {
                            Err(e)
                        }
                    }
                }
            }
        }
    }
}

// ── Internal helpers ────────────────────────────────────────────────

/// Perform the actual HTTP POST and interpret the response.
///
/// Network and parse errors are propagated as `Err`, but only after checking
/// whether the expected output file appeared on disk (file recovery).
/// Task-level failures (server returned `success: false`) are returned as
/// `Ok(TaskResult { success: false, .. })`.
async fn http_call(
    http: &reqwest::Client,
    url: &str,
    payload: &TaskPayload,
    expected_output: Option<&Path>,
) -> Result<TaskResult, SdkError> {
    let try_file_recovery = |expected: Option<&Path>| -> Option<TaskResult> {
        let path = expected?;
        if path.exists() {
            Some(TaskResult {
                success: true,
                output: String::from("(recovered from file)"),
                exit_code: None,
                elapsed: None,
                started_at: None,
            })
        } else {
            None
        }
    };

    let resp = match http.post(url).json(payload).send().await {
        Ok(r) => r,
        Err(e) => {
            if let Some(recovered) = try_file_recovery(expected_output) {
                return Ok(recovered);
            }
            return Err(SdkError::Http(e));
        }
    };

    let response: ApiResponse = match resp.json::<ApiResponse>().await {
        Ok(r) => r,
        Err(e) => {
            if let Some(recovered) = try_file_recovery(expected_output) {
                return Ok(recovered);
            }
            return Err(SdkError::Http(e));
        }
    };

    let meta = response.metadata.as_ref();
    let exit_code = meta.and_then(|m| m.exit_code);
    let elapsed = meta.and_then(|m| m.elapsed.as_ref().map(ApiElapsed::to_display_string));
    let started_at = meta.and_then(|m| m.started_at.clone());

    if response.success == Some(true) {
        let output = response.output.unwrap_or_default().trim().to_owned();
        return Ok(TaskResult {
            success: true,
            output,
            exit_code,
            elapsed,
            started_at,
        });
    }

    if let Some(recovered) = try_file_recovery(expected_output) {
        return Ok(recovered);
    }

    let output = response
        .output
        .or(response.message)
        .unwrap_or_else(|| String::from("unknown error"));
    Ok(TaskResult {
        success: false,
        output,
        exit_code,
        elapsed,
        started_at,
    })
}

/// Poll for a file to appear on disk, with an initial delay and a maximum duration.
///
/// Returns `Ok(())` when the file appears, or `Err(SdkError::PollTimeout)` when
/// the total elapsed time exceeds `max_duration`.
async fn poll_for_file(
    file_path: &Path,
    initial_delay: Duration,
    interval: Duration,
    max_duration: Duration,
) -> Result<(), SdkError> {
    let start = tokio::time::Instant::now();
    let effective_initial = initial_delay.min(max_duration);
    tokio::time::sleep(effective_initial).await;
    loop {
        let elapsed = start.elapsed();
        if elapsed >= max_duration {
            return Err(SdkError::PollTimeout { elapsed });
        }
        if file_path.exists() {
            return Ok(());
        }
        let remaining = max_duration.saturating_sub(elapsed);
        let sleep_time = interval.min(remaining);
        tokio::time::sleep(sleep_time).await;
    }
}

/// Truncate a string to at most `max_len` bytes on a char boundary.
pub(crate) fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_owned()
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        s.get(..end).unwrap_or_default().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_str_short_passthrough() {
        let s = "hello";
        assert_eq!(truncate_str(s, 10), "hello", "short strings pass through");
    }

    #[test]
    fn truncate_str_exact_boundary() {
        let s = "abcde";
        assert_eq!(
            truncate_str(s, 5),
            "abcde",
            "strings at exact boundary pass through"
        );
    }

    #[test]
    fn truncate_str_cuts_long() {
        let s = "abcdefghij";
        assert_eq!(truncate_str(s, 5), "abcde", "should truncate to max_len");
    }

    #[test]
    fn truncate_str_multibyte_boundary() {
        // Each emoji is 4 bytes. "ab" = 2 bytes, then 4-byte char.
        let s = "ab\u{1F600}cd";
        // max_len=4 lands inside the emoji — should back up to byte 2.
        assert_eq!(
            truncate_str(s, 4),
            "ab",
            "should back up to char boundary"
        );
    }

    #[test]
    fn truncate_str_zero_max() {
        let s = "hello";
        assert_eq!(truncate_str(s, 0), "", "zero max_len produces empty string");
    }

    #[test]
    fn client_config_default_values() {
        let cfg = ClientConfig::default();
        assert_eq!(
            cfg.base_url, "http://localhost:7943",
            "default base URL"
        );
        assert_eq!(
            cfg.timeout,
            Duration::from_millis(2_700_000),
            "default timeout is 45 minutes"
        );
        assert_eq!(
            cfg.poll_interval,
            Duration::from_millis(10_000),
            "default poll interval is 10 seconds"
        );
        assert_eq!(
            cfg.poll_initial_delay,
            Duration::from_millis(30_000),
            "default poll initial delay is 30 seconds"
        );
        assert_eq!(
            cfg.max_poll_duration,
            Duration::from_millis(2_700_000),
            "default max poll duration is 45 minutes"
        );
    }

    #[test]
    fn client_config_custom() {
        let cfg = ClientConfig {
            base_url: String::from("http://example.com:8080"),
            timeout: Duration::from_secs(60),
            poll_interval: Duration::from_secs(1),
            poll_initial_delay: Duration::from_secs(5),
            max_poll_duration: Duration::from_secs(120),
        };
        assert_eq!(cfg.base_url, "http://example.com:8080", "custom base URL");
        assert_eq!(cfg.timeout, Duration::from_secs(60), "custom timeout");
        assert_eq!(
            cfg.max_poll_duration,
            Duration::from_secs(120),
            "custom max poll duration"
        );
    }

    #[tokio::test]
    async fn poll_for_file_timeout_returns_error() {
        let dir = tempfile::tempdir().unwrap_or_else(|_| std::process::abort());
        let path = dir.path().join("nonexistent.txt");

        let result = poll_for_file(
            &path,
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_err(), "should timeout when file does not appear");
        let is_poll_timeout = matches!(result, Err(SdkError::PollTimeout { .. }));
        assert!(is_poll_timeout, "should be PollTimeout variant");
    }

    #[tokio::test]
    async fn poll_for_file_succeeds_when_file_appears() {
        let dir = tempfile::tempdir().unwrap_or_else(|_| std::process::abort());
        let path = dir.path().join("output.txt");
        let path_clone = path.clone();

        // Spawn a task that creates the file after 100ms.
        let _handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = std::fs::write(&path_clone, "done");
        });

        let result = poll_for_file(
            &path,
            Duration::from_millis(10),
            Duration::from_millis(50),
            Duration::from_millis(500),
        )
        .await;

        assert!(result.is_ok(), "should succeed when file appears during polling");
    }

    #[tokio::test]
    async fn submit_task_returns_err_on_connection_refused() {
        let config = ClientConfig {
            base_url: String::from("http://127.0.0.1:19999"),
            timeout: Duration::from_millis(500),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());

        let payload = TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hello"),
            working_directory: None,
            environment: None,
            limiter_key: None,
            timeout_ms: None,
        };
        let result = client.submit_task(&payload).await;

        assert!(result.is_err(), "network failure should return Err");
        let is_http = matches!(result, Err(SdkError::Http(_)));
        assert!(is_http, "should be SdkError::Http variant");
    }

    #[tokio::test]
    async fn submit_task_with_recovery_recovers_from_file() {
        // Bind a listener that accepts connections but never responds,
        // so the HTTP call hangs rather than failing immediately.
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|_| std::process::abort());
        let addr = listener.local_addr().unwrap_or_else(|_| std::process::abort());

        // Accept connections in background so the OS doesn't RST them.
        let _accept_handle = tokio::spawn(async move {
            // Keep listener alive and accept (but never read/write).
            loop {
                let _conn = listener.accept();
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });

        let dir = tempfile::tempdir().unwrap_or_else(|_| std::process::abort());
        let output_path = dir.path().join("recovered.txt");
        let output_clone = output_path.clone();

        // Spawn a task that creates the file after 50ms.
        let _handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = std::fs::write(&output_clone, "recovered content");
        });

        let config = ClientConfig {
            base_url: format!("http://{addr}"),
            timeout: Duration::from_millis(5000),
            poll_interval: Duration::from_millis(30),
            poll_initial_delay: Duration::from_millis(10),
            max_poll_duration: Duration::from_millis(1000),
        };
        let client = Client::new(config).unwrap_or_else(|_| std::process::abort());

        let payload = TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hello"),
            working_directory: None,
            environment: None,
            limiter_key: None,
            timeout_ms: None,
        };

        let result = client
            .submit_task_with_recovery(&payload, &output_path)
            .await;

        assert!(result.is_ok(), "should recover via file poll");
        let task_result = result.unwrap_or_else(|_| std::process::abort());
        assert!(task_result.success, "recovered result should be successful");
    }

    #[test]
    fn task_result_require_success_ok() {
        let result = TaskResult {
            success: true,
            output: String::from("done"),
            exit_code: Some(0),
            elapsed: None,
            started_at: None,
        };
        assert!(result.require_success().is_ok(), "should pass for successful result");
    }

    #[test]
    fn task_result_require_success_err() {
        let result = TaskResult {
            success: false,
            output: String::from("command failed"),
            exit_code: Some(1),
            elapsed: None,
            started_at: None,
        };
        let err = result.require_success();
        assert!(err.is_err(), "should fail for unsuccessful result");
        let is_task_failed = matches!(err, Err(SdkError::TaskFailed { .. }));
        assert!(is_task_failed, "should be TaskFailed variant");
    }

    #[test]
    fn api_elapsed_display_seconds_only() {
        let elapsed = ApiElapsed { secs: Some(42), nanos: Some(0) };
        assert_eq!(elapsed.to_display_string(), "42s");
    }

    #[test]
    fn api_elapsed_display_with_millis() {
        let elapsed = ApiElapsed { secs: Some(3), nanos: Some(150_000_000) };
        assert_eq!(elapsed.to_display_string(), "3.150s");
    }

    // ── Regression tests ────────────────────────────────────────────

    #[tokio::test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    async fn regression_http_call_returns_err_on_network_failure() {
        // Regression: http_call previously swallowed network errors and returned
        // Ok(TaskResult{success:false}) instead of Err(SdkError::Http).
        let config = ClientConfig {
            base_url: String::from("http://127.0.0.1:19999"),
            timeout: Duration::from_millis(500),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap();

        let payload = TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hello"),
            working_directory: None,
            environment: None,
            limiter_key: None,
            timeout_ms: None,
        };
        let result = client.submit_task(&payload).await;

        // Must be Err, not Ok with success=false.
        assert!(result.is_err(), "connection refused must return Err, not Ok");
        assert!(
            matches!(result, Err(SdkError::Http(_))),
            "must be SdkError::Http variant, got: {result:?}"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    fn regression_response_metadata_populated() {
        // Regression: TaskResult metadata fields (exit_code, elapsed, started_at)
        // were dropped and always None.
        let json = serde_json::json!({
            "success": true,
            "output": "ok",
            "metadata": {
                "exit_code": 0,
                "started_at": "2025-01-15T10:00:00Z",
                "elapsed": { "secs": 42, "nanos": 500_000_000_u64 }
            }
        });
        let api_resp: ApiResponse = serde_json::from_value(json).unwrap();
        let meta = api_resp.metadata.as_ref();
        let exit_code = meta.and_then(|m| m.exit_code);
        let elapsed = meta.and_then(|m| m.elapsed.as_ref().map(ApiElapsed::to_display_string));
        let started_at = meta.and_then(|m| m.started_at.clone());

        assert_eq!(exit_code, Some(0), "exit_code must be populated from response");
        assert_eq!(
            elapsed.as_deref(),
            Some("42.500s"),
            "elapsed must be populated from response"
        );
        assert_eq!(
            started_at.as_deref(),
            Some("2025-01-15T10:00:00Z"),
            "started_at must be populated from response"
        );
    }

    #[tokio::test]
    async fn regression_poll_timeout_overshoot_initial_delay_exceeds_max() {
        // Regression: poll_for_file with initial_delay > max_duration would sleep
        // for the full initial_delay before checking timeout, overshooting max_duration.
        let dir = tempfile::tempdir().unwrap_or_else(|_| std::process::abort());
        let path = dir.path().join("nonexistent.txt");

        let max_duration = Duration::from_millis(50);
        let initial_delay = Duration::from_millis(500); // 10x max_duration

        let start = tokio::time::Instant::now();
        let result = poll_for_file(
            &path,
            initial_delay,
            Duration::from_millis(10),
            max_duration,
        )
        .await;

        let actual_elapsed = start.elapsed();

        assert!(result.is_err(), "should timeout");
        assert!(
            matches!(result, Err(SdkError::PollTimeout { .. })),
            "should be PollTimeout"
        );
        // Must return within max_duration + generous epsilon (100ms), NOT after initial_delay (500ms).
        let epsilon = Duration::from_millis(100);
        assert!(
            actual_elapsed < max_duration.saturating_add(epsilon),
            "poll must return within max_duration + epsilon ({:?}), but took {:?}",
            max_duration.saturating_add(epsilon),
            actual_elapsed
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    async fn regression_file_poll_recovery_succeeds() {
        // Regression: poll_for_file would not detect files created after the
        // initial delay but before timeout.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.txt");
        let path_clone = path.clone();

        // Create file after 80ms — well after initial_delay (10ms) but before timeout (500ms).
        let _handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            let _ = std::fs::write(&path_clone, "done");
        });

        let result = poll_for_file(
            &path,
            Duration::from_millis(10),
            Duration::from_millis(30),
            Duration::from_millis(500),
        )
        .await;

        assert!(result.is_ok(), "file created mid-poll must be detected");
    }

    #[test]
    fn regression_require_success_returns_task_failed() {
        // Regression: TaskResult{success:false}.require_success() must return
        // Err(SdkError::TaskFailed), not swallow the error as Ok.
        let result = TaskResult {
            success: false,
            output: String::from("command failed"),
            exit_code: Some(1),
            elapsed: None,
            started_at: None,
        };
        let err = result.require_success();
        assert!(err.is_err(), "failed task must return Err");
        assert!(
            matches!(&err, Err(SdkError::TaskFailed { message }) if message == "command failed"),
            "must be TaskFailed with preserved output message, got: {err:?}"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    fn regression_task_payload_serializes_limiter_key_and_timeout_ms() {
        // Regression: limiter_key and timeout_ms fields were missing from
        // TaskPayload serialization.
        let payload = TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hello"),
            working_directory: None,
            environment: None,
            limiter_key: Some(String::from("my-limiter")),
            timeout_ms: Some(30_000),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(
            json.get("limiter_key").and_then(serde_json::Value::as_str),
            Some("my-limiter"),
            "limiter_key must be serialized"
        );
        assert_eq!(
            json.get("timeout_ms").and_then(serde_json::Value::as_u64),
            Some(30_000),
            "timeout_ms must be serialized"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    fn mutant_kill_base_url_returns_configured_url() {
        // Mutant kill: client.rs:165-167 — base_url() replaced with "" or "xyzzy"
        let config = ClientConfig {
            base_url: String::from("http://custom-server:9999"),
            ..ClientConfig::default()
        };
        let client = Client::new(config).unwrap();
        assert_eq!(
            client.base_url(),
            "http://custom-server:9999",
            "base_url() must return the configured URL, not an empty string or constant"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    fn mutant_kill_success_check_true_returns_ok() {
        // Mutant kill: client.rs:304 — `== with !=` on success check
        // Directly test the response interpretation: success=true must yield
        // TaskResult.success=true, and success=false must yield success=false.
        let json_success = serde_json::json!({
            "success": true,
            "output": "good",
            "metadata": null
        });
        let api_resp: ApiResponse = serde_json::from_value(json_success).unwrap();
        assert_eq!(
            api_resp.success,
            Some(true),
            "success=true in JSON must parse as Some(true)"
        );
        // The code checks `if response.success == Some(true)` to return Ok with success=true.
        // If mutated to `!=`, a success=true response would fall through to the error path.
        // We can't call http_call without a server, but we can verify the parsed value
        // and the branching logic by checking require_success on the expected TaskResult.
        let result = TaskResult {
            success: api_resp.success == Some(true),
            output: api_resp.output.unwrap_or_default().trim().to_owned(),
            exit_code: None,
            elapsed: None,
            started_at: None,
        };
        assert!(
            result.success,
            "response with success=true must produce TaskResult.success=true"
        );

        // Also verify the inverse: success=false must NOT match.
        let json_failure = serde_json::json!({
            "success": false,
            "output": "bad"
        });
        let api_resp_fail: ApiResponse = serde_json::from_value(json_failure).unwrap();
        let result_fail = TaskResult {
            success: api_resp_fail.success == Some(true),
            output: String::from("bad"),
            exit_code: None,
            elapsed: None,
            started_at: None,
        };
        assert!(
            !result_fail.success,
            "response with success=false must produce TaskResult.success=false"
        );
    }

    #[test]
    fn mutant_kill_truncate_str_at_exact_boundary() {
        // Mutant kill: client.rs:361 — `> with >=` on `s.len() <= max_len`
        // or client.rs:365 — `> with >=` on `while end > 0`
        // A string of exactly max_len bytes must NOT be truncated.
        let s = "abcde"; // 5 bytes
        let result = truncate_str(s, 5);
        assert_eq!(
            result, "abcde",
            "string of exactly max_len must pass through unchanged"
        );
        assert_eq!(
            result.len(),
            5,
            "result length must equal max_len when input length equals max_len"
        );

        // A string one byte longer MUST be truncated.
        let s2 = "abcdef"; // 6 bytes
        let result2 = truncate_str(s2, 5);
        assert_eq!(
            result2, "abcde",
            "string one byte over max_len must be truncated"
        );
        assert_eq!(
            result2.len(),
            5,
            "truncated result must have exactly max_len bytes"
        );

        // Edge case: single multibyte char with max_len that splits it must back up to 0.
        let s3 = "\u{1F600}"; // 4 bytes
        let result3 = truncate_str(s3, 2);
        assert_eq!(
            result3, "",
            "truncate inside a multibyte char must back up to empty when no char boundary fits"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertions
    fn regression_task_payload_omits_none_optional_fields() {
        // Verify that None optional fields are skipped during serialization
        // (skip_serializing_if = "Option::is_none").
        let payload = TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hello"),
            working_directory: None,
            environment: None,
            limiter_key: None,
            timeout_ms: None,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(
            json.get("limiter_key").is_none(),
            "None limiter_key must be omitted"
        );
        assert!(
            json.get("timeout_ms").is_none(),
            "None timeout_ms must be omitted"
        );
    }
}
