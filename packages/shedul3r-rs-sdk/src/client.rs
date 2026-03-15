//! HTTP client for the shedul3r task execution API.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: String::from("http://localhost:7943"),
            timeout: Duration::from_millis(2_700_000),
            poll_interval: Duration::from_millis(10_000),
            poll_initial_delay: Duration::from_millis(30_000),
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
    pub environment: Option<HashMap<String, String>>,
}

/// Response from shedul3r after task completion.
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Whether the task completed successfully.
    pub success: bool,
    /// Task output text (or error message on failure).
    pub output: String,
}

/// Raw JSON response from the shedul3r API.
#[derive(Deserialize)]
struct ApiResponse {
    success: Option<bool>,
    output: Option<String>,
    message: Option<String>,
}

impl Client {
    /// Create a new client with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new(config: ClientConfig) -> anyhow::Result<Self> {
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
    pub fn with_defaults() -> anyhow::Result<Self> {
        Self::new(ClientConfig::default())
    }

    /// Submit a task and wait for the HTTP response.
    ///
    /// # Errors
    ///
    /// Returns an error only for unrecoverable issues (e.g. serialisation
    /// failure). HTTP failures are reported via [`TaskResult::success`] `= false`.
    pub async fn submit_task(&self, payload: &TaskPayload) -> anyhow::Result<TaskResult> {
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
    /// Returns an error only for unrecoverable issues. Transient failures
    /// are reported via [`TaskResult::success`] `= false`.
    pub async fn submit_task_with_recovery(
        &self,
        payload: &TaskPayload,
        expected_output: &Path,
    ) -> anyhow::Result<TaskResult> {
        // Remove stale output so the poller only fires on fresh writes.
        let _ = std::fs::remove_file(expected_output);

        let url = format!("{}/api/tasks", self.config.base_url);
        let output_owned = expected_output.to_path_buf();

        let http_future = http_call(&self.http, &url, payload, Some(&output_owned));
        let poll_future = poll_for_file(
            &output_owned,
            self.config.poll_initial_delay,
            self.config.poll_interval,
        );

        tokio::select! {
            biased;
            result = http_future => result,
            poll_result = poll_future => {
                match poll_result {
                    Ok(()) => Ok(TaskResult {
                        success: true,
                        output: String::from("(recovered from file poll)"),
                    }),
                    Err(e) => {
                        // Poll failed — check if the file appeared anyway.
                        if output_owned.exists() {
                            Ok(TaskResult {
                                success: true,
                                output: String::from("(recovered from file)"),
                            })
                        } else {
                            Ok(TaskResult {
                                success: false,
                                output: format!("both HTTP and file poll failed: {e}"),
                            })
                        }
                    }
                }
            }
        }
    }
}

// ── Internal helpers ────────────────────────────────────────────────

/// Perform the actual HTTP POST and interpret the response.
async fn http_call(
    http: &reqwest::Client,
    url: &str,
    payload: &TaskPayload,
    expected_output: Option<&Path>,
) -> anyhow::Result<TaskResult> {
    let result = http.post(url).json(payload).send().await;

    let try_file_recovery = |expected: Option<&Path>| -> Option<TaskResult> {
        let path = expected?;
        if path.exists() {
            Some(TaskResult {
                success: true,
                output: String::from("(recovered from file)"),
            })
        } else {
            None
        }
    };

    let error_result = |err: &dyn std::fmt::Display, expected: Option<&Path>| -> TaskResult {
        if let Some(recovered) = try_file_recovery(expected) {
            return recovered;
        }
        let msg = err.to_string();
        TaskResult {
            success: false,
            output: truncate_str(&msg, 500),
        }
    };

    match result {
        Ok(resp) => {
            let parsed = resp.json::<ApiResponse>().await;
            match parsed {
                Ok(response) => {
                    if response.success == Some(true) {
                        let output = response.output.unwrap_or_default().trim().to_owned();
                        return Ok(TaskResult {
                            success: true,
                            output,
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
                    })
                }
                Err(err) => Ok(error_result(&err, expected_output)),
            }
        }
        Err(err) => Ok(error_result(&err, expected_output)),
    }
}

/// Poll for a file to appear on disk, with an initial delay.
async fn poll_for_file(
    file_path: &Path,
    initial_delay: Duration,
    interval: Duration,
) -> anyhow::Result<()> {
    tokio::time::sleep(initial_delay).await;
    loop {
        if file_path.exists() {
            return Ok(());
        }
        tokio::time::sleep(interval).await;
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
    }

    #[test]
    fn client_config_custom() {
        let cfg = ClientConfig {
            base_url: String::from("http://example.com:8080"),
            timeout: Duration::from_secs(60),
            poll_interval: Duration::from_secs(1),
            poll_initial_delay: Duration::from_secs(5),
        };
        assert_eq!(cfg.base_url, "http://example.com:8080", "custom base URL");
        assert_eq!(cfg.timeout, Duration::from_secs(60), "custom timeout");
    }
}
