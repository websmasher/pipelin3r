//! HTTP client for the shedul3r task execution API.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::SdkError;

/// Type alias for environment variable maps to reduce type complexity.
pub type EnvironmentMap = BTreeMap<String, String>;

mod async_poll;
pub use async_poll::AsyncTaskStatus;

#[cfg(test)]
mod tests;

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
    pub environment: Option<EnvironmentMap>,
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
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiResponse {
    pub(crate) success: Option<bool>,
    pub(crate) output: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) metadata: Option<ApiResponseMetadata>,
}

/// Metadata nested inside [`ApiResponse`].
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiResponseMetadata {
    pub(crate) started_at: Option<String>,
    pub(crate) elapsed: Option<ApiElapsed>,
    pub(crate) exit_code: Option<i32>,
}

/// Elapsed time from the server response.
///
/// The server serializes `Duration` as fractional seconds (e.g., `420.283`).
/// We accept both a plain float and a `{ secs, nanos }` struct for
/// backwards compatibility.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum ApiElapsed {
    /// Fractional seconds (current shedul3r format).
    Float(f64),
    /// Struct with secs/nanos (legacy format).
    Struct {
        secs: Option<u64>,
        nanos: Option<u32>,
    },
}

impl ApiElapsed {
    /// Format the elapsed duration as a human-readable string.
    pub(crate) fn to_display_string(&self) -> String {
        match self {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                clippy::as_conversions,
                reason = "elapsed seconds are always small positive values; precision loss is acceptable for display"
            )]
            Self::Float(secs) => {
                let whole = *secs as u64;
                let frac = *secs - (whole as f64);
                let millis = (frac * 1000.0) as u64;
                if millis == 0 {
                    format!("{whole}s")
                } else {
                    format!("{whole}.{millis:03}s")
                }
            }
            Self::Struct { secs, nanos } => {
                let s = secs.unwrap_or(0);
                let n = nanos.unwrap_or(0);
                if n == 0 {
                    format!("{s}s")
                } else {
                    let millis = n.saturating_div(1_000_000);
                    format!("{s}.{millis:03}s")
                }
            }
        }
    }
}

impl Client {
    /// Create a new client with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    #[allow(clippy::disallowed_methods)] // SDK client library: reqwest::Client construction is core functionality
    pub fn new(config: ClientConfig) -> Result<Self, SdkError> {
        let http = reqwest::Client::builder().timeout(config.timeout).build()?;
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
        let _ = crate::fs::remove_file(expected_output);

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
#[allow(clippy::disallowed_methods)] // SDK client: thin HTTP wrapper, validation is caller's responsibility
#[allow(
    clippy::too_many_lines,
    reason = "HTTP call with recovery has sequential phases"
)]
#[allow(
    clippy::print_stderr,
    reason = "SDK diagnostic output — no tracing dependency"
)]
pub(crate) async fn http_call(
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

    // Read raw bytes first to diagnose decode failures.
    let raw_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[shedul3r-sdk] failed to read response bytes: {e}");
            if let Some(recovered) = try_file_recovery(expected_output) {
                return Ok(recovered);
            }
            return Err(SdkError::Http(e));
        }
    };

    let response: ApiResponse = match serde_json::from_slice(&raw_bytes) {
        Ok(r) => r,
        Err(e) => {
            let body_len = raw_bytes.len();
            let preview = String::from_utf8_lossy(
                raw_bytes
                    .get(..raw_bytes.len().min(300))
                    .unwrap_or(&raw_bytes),
            );
            eprintln!(
                "[shedul3r-sdk] JSON parse failed: {e} | body: {body_len} bytes | preview: {preview}"
            );
            if let Some(recovered) = try_file_recovery(expected_output) {
                return Ok(recovered);
            }
            return Err(SdkError::TaskFailed {
                message: format!("response JSON parse error: {e} (body: {body_len} bytes)"),
            });
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
pub(crate) async fn poll_for_file(
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
