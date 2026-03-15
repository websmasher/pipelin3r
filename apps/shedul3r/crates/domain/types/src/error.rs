//! Domain error types for the schedulr engine.

/// Errors that can occur during task scheduling and execution.
#[derive(Debug, thiserror::Error)]
pub enum SchedulrError {
    /// A resilience component (rate limiter, circuit breaker, bulkhead, retry) failed.
    #[error(transparent)]
    Resilience(#[from] limit3r::Limit3rError),

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
