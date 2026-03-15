//! Typed error enum for the shedul3r SDK.

use std::time::Duration;

/// Errors that can occur when using the shedul3r SDK.
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    /// An HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// A scheduled task reported failure.
    #[error("task failed: {message}")]
    TaskFailed {
        /// Human-readable failure message from the server.
        message: String,
    },
    /// File polling exceeded the maximum allowed duration.
    #[error("file poll timeout after {elapsed:?}")]
    PollTimeout {
        /// How long polling ran before giving up.
        elapsed: Duration,
    },
    /// A bundle upload, download, or delete operation failed.
    #[error("bundle operation failed: {0}")]
    Bundle(String),
    /// JSON serialisation or deserialisation failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A filesystem I/O operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
