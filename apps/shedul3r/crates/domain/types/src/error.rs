//! Domain error types for the schedulr engine.

/// Errors that can occur during task scheduling and execution.
#[derive(Debug, thiserror::Error)]
pub enum SchedulrError {
    /// The rate limiter rejected the request because permits are exhausted.
    #[error("Rate limit exceeded for key '{key}'")]
    RateLimitExceeded {
        /// The limiter key that was rate-limited.
        key: String,
    },

    /// The circuit breaker is open and rejecting requests.
    #[error("Circuit breaker open for key '{key}'")]
    CircuitOpen {
        /// The limiter key whose circuit is open.
        key: String,
    },

    /// All retry attempts have been exhausted without success.
    #[error("All {attempts} retry attempts exhausted")]
    RetryExhausted {
        /// Total number of attempts that were made.
        attempts: u32,
    },

    /// The bulkhead is full and cannot accept more concurrent requests.
    #[error("Bulkhead full for key '{key}'")]
    BulkheadFull {
        /// The limiter key whose bulkhead is saturated.
        key: String,
    },

    /// The task definition could not be parsed or is invalid.
    #[error("{0}")]
    TaskDefinition(String),

    /// The subprocess exited with a non-zero exit code.
    #[error("Exit {exit_code}: {message}")]
    Subprocess {
        /// The process exit code.
        exit_code: i32,
        /// Description of the failure.
        message: String,
    },

    /// An I/O error occurred during execution.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
