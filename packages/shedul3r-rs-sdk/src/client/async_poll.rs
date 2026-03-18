//! Async task submission and polling for the shedul3r API.

use std::time::Duration;

use serde::Deserialize;

use super::{ApiElapsed, ApiResponse, Client, TaskPayload, TaskResult};
use crate::error::SdkError;

/// Response from `POST /api/tasks/async`.
#[derive(Deserialize)]
pub(super) struct AsyncSubmitResponse {
    pub(super) task_id: String,
}

/// Status of an asynchronously submitted task.
///
/// Returned by [`Client::get_task_status`].
#[derive(Debug, Clone, Deserialize)]
#[allow(
    clippy::partial_pub_fields,
    reason = "result uses pub(crate) type ApiResponse — access via into_task_result()"
)]
pub struct AsyncTaskStatus {
    /// Current status: `"running"`, `"completed"`, or `"failed"`.
    pub status: String,
    /// The task result, present when `status` is `"completed"`.
    pub(crate) result: Option<ApiResponse>,
    /// Error message, present when `status` is `"failed"`.
    pub error: Option<String>,
}

impl AsyncTaskStatus {
    /// Convert the embedded result into a [`TaskResult`], if present.
    #[must_use]
    pub fn into_task_result(self) -> Option<TaskResult> {
        self.result.map(api_response_to_result)
    }
}

/// Convert an [`ApiResponse`] into a [`TaskResult`].
pub(super) fn api_response_to_result(response: ApiResponse) -> TaskResult {
    let meta = response.metadata.as_ref();
    let exit_code = meta.and_then(|m| m.exit_code);
    let elapsed = meta.and_then(|m| m.elapsed.as_ref().map(ApiElapsed::to_display_string));
    let started_at = meta.and_then(|m| m.started_at.clone());

    if response.success == Some(true) {
        let output = response.output.unwrap_or_default().trim().to_owned();
        return TaskResult {
            success: true,
            output,
            exit_code,
            elapsed,
            started_at,
        };
    }

    let output = response
        .output
        .or(response.message)
        .unwrap_or_else(|| String::from("unknown error"));
    TaskResult {
        success: false,
        output,
        exit_code,
        elapsed,
        started_at,
    }
}

impl Client {
    /// Submit a task asynchronously. Returns the server-assigned task ID
    /// immediately without waiting for completion.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Http`] for network failures.
    /// Returns [`SdkError::TaskFailed`] if the server returns a non-2xx status
    /// or the response cannot be parsed.
    #[allow(clippy::disallowed_methods)] // SDK client: thin HTTP wrapper
    #[allow(
        clippy::print_stderr,
        reason = "SDK diagnostic output — no tracing dependency"
    )]
    pub async fn submit_task_async(&self, payload: &TaskPayload) -> Result<String, SdkError> {
        let url = format!("{}/api/tasks/async", self.config.base_url);
        let resp = self.http.post(&url).json(payload).send().await?;
        let status_code = resp.status();
        let raw_bytes = resp.bytes().await?;

        if !status_code.is_success() {
            let preview = String::from_utf8_lossy(
                raw_bytes
                    .get(..raw_bytes.len().min(300))
                    .unwrap_or(&raw_bytes),
            );
            eprintln!(
                "[shedul3r-sdk] async submit failed with HTTP {status_code} | preview: {preview}"
            );
            return Err(SdkError::TaskFailed {
                message: format!("async submit failed with HTTP {status_code}"),
            });
        }

        let body: AsyncSubmitResponse = serde_json::from_slice(&raw_bytes).map_err(|e| {
            let preview = String::from_utf8_lossy(
                raw_bytes
                    .get(..raw_bytes.len().min(300))
                    .unwrap_or(&raw_bytes),
            );
            eprintln!("[shedul3r-sdk] async submit JSON parse failed: {e} | preview: {preview}");
            SdkError::TaskFailed {
                message: format!("async submit response parse error: {e}"),
            }
        })?;
        Ok(body.task_id)
    }

    /// Poll for the status of an asynchronously submitted task.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Http`] for network failures.
    /// Returns [`SdkError::TaskFailed`] if the task ID is not found (HTTP 404),
    /// the server returns a non-2xx status, or the response cannot be parsed.
    #[allow(clippy::disallowed_methods)] // SDK client: thin HTTP wrapper
    #[allow(
        clippy::print_stderr,
        reason = "SDK diagnostic output — no tracing dependency"
    )]
    pub async fn get_task_status(&self, task_id: &str) -> Result<AsyncTaskStatus, SdkError> {
        let url = format!("{}/api/tasks/async/{task_id}", self.config.base_url);
        let resp = self.http.get(&url).send().await?;
        let status_code = resp.status();
        let raw_bytes = resp.bytes().await?;

        if status_code == reqwest::StatusCode::NOT_FOUND {
            return Err(SdkError::TaskFailed {
                message: format!("task not found: {task_id}"),
            });
        }

        if !status_code.is_success() {
            let preview = String::from_utf8_lossy(
                raw_bytes
                    .get(..raw_bytes.len().min(300))
                    .unwrap_or(&raw_bytes),
            );
            eprintln!(
                "[shedul3r-sdk] task status failed with HTTP {status_code} | preview: {preview}"
            );
            return Err(SdkError::TaskFailed {
                message: format!("task status request failed with HTTP {status_code}"),
            });
        }

        let body: AsyncTaskStatus = serde_json::from_slice(&raw_bytes).map_err(|e| {
            let preview = String::from_utf8_lossy(
                raw_bytes
                    .get(..raw_bytes.len().min(300))
                    .unwrap_or(&raw_bytes),
            );
            eprintln!("[shedul3r-sdk] task status JSON parse failed: {e} | preview: {preview}");
            SdkError::TaskFailed {
                message: format!("task status response parse error: {e}"),
            }
        })?;
        Ok(body)
    }

    /// Submit a task asynchronously and poll until it completes.
    ///
    /// This is the preferred method for remote execution — it avoids holding
    /// a long-lived HTTP connection open. The poll interval is configured via
    /// [`ClientConfig::poll_interval`]. The total polling time is bounded by
    /// [`ClientConfig::max_poll_duration`].
    ///
    /// Transient server errors (5xx) during polling are retried up to 3 times
    /// before the error is propagated. Client errors (4xx) are not retried.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Http`] for network failures.
    /// Returns [`SdkError::TaskFailed`] if the task fails on the server.
    /// Returns [`SdkError::PollTimeout`] if polling exceeds `max_poll_duration`.
    #[allow(
        clippy::print_stderr,
        reason = "SDK diagnostic output — no tracing dependency"
    )]
    pub async fn submit_task_poll(&self, payload: &TaskPayload) -> Result<TaskResult, SdkError> {
        /// Maximum number of consecutive transient (5xx) errors before giving up.
        const MAX_TRANSIENT_RETRIES: u32 = 3;

        let task_id = self.submit_task_async(payload).await?;
        let deadline = tokio::time::Instant::now()
            .checked_add(self.config.max_poll_duration)
            .unwrap_or_else(|| {
                // max_poll_duration is so large it overflows Instant — use a far-future deadline
                tokio::time::Instant::now()
                    .checked_add(Duration::from_secs(86400))
                    .unwrap_or_else(tokio::time::Instant::now)
            });
        let mut transient_errors: u32 = 0;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(SdkError::PollTimeout {
                    elapsed: self.config.max_poll_duration,
                });
            }

            tokio::time::sleep(self.config.poll_interval).await;

            let status = match self.get_task_status(&task_id).await {
                Ok(s) => {
                    transient_errors = 0;
                    s
                }
                Err(SdkError::TaskFailed { ref message })
                    if message.contains("HTTP 5") && transient_errors < MAX_TRANSIENT_RETRIES =>
                {
                    transient_errors = transient_errors.saturating_add(1);
                    eprintln!(
                        "[shedul3r-sdk] transient error during poll \
                         (attempt {transient_errors}/{MAX_TRANSIENT_RETRIES}): {message}"
                    );
                    continue;
                }
                Err(e) => return Err(e),
            };

            match status.status.as_str() {
                "running" | "pending" => {}
                "completed" => {
                    return if let Some(response) = status.result {
                        Ok(api_response_to_result(response))
                    } else {
                        Ok(TaskResult {
                            success: true,
                            output: String::new(),
                            exit_code: None,
                            elapsed: None,
                            started_at: None,
                        })
                    };
                }
                "failed" => {
                    return Err(SdkError::TaskFailed {
                        message: status
                            .error
                            .unwrap_or_else(|| String::from("unknown error")),
                    });
                }
                other => {
                    return Err(SdkError::TaskFailed {
                        message: format!("unexpected task status: {other}"),
                    });
                }
            }
        }
    }
}
