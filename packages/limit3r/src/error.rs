//! Error types for resilience pattern failures.

/// Errors produced by resilience components (rate limiter, circuit breaker,
/// bulkhead, retry).
#[derive(Debug, thiserror::Error)]
pub enum Limit3rError {
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
}
