//! Outbound port traits: interfaces for subprocess execution and resilience components.

use core::future::Future;

use domain_types::{
    BulkheadConfig, CircuitBreakerConfig, RateLimitConfig, RetryConfig, SchedulrError,
    SubprocessCommand, SubprocessResult,
};

/// Runs shell commands as subprocesses.
pub trait SubprocessRunner: Send + Sync {
    /// Execute the given command and return its result.
    fn run(
        &self,
        command: SubprocessCommand,
    ) -> impl Future<Output = Result<SubprocessResult, SchedulrError>> + Send;
}

/// Rate limiter — acquires permits per key within a configured time window.
pub trait RateLimiter: Send + Sync {
    /// Attempt to acquire a rate limit permit for the given key.
    ///
    /// Returns `Ok(())` if a permit was acquired, or
    /// [`SchedulrError::RateLimitExceeded`] if the limit is exhausted.
    fn acquire_permission(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> impl Future<Output = Result<(), SchedulrError>> + Send;
}

/// Circuit breaker — tracks failure rates per key and opens the circuit
/// when the failure threshold is exceeded.
pub trait CircuitBreaker: Send + Sync {
    /// Check whether the circuit is closed (requests permitted).
    ///
    /// # Errors
    ///
    /// Returns [`SchedulrError::CircuitOpen`] if the circuit is open.
    fn check_permitted(
        &self,
        key: &str,
        config: &CircuitBreakerConfig,
    ) -> Result<(), SchedulrError>;

    /// Record a successful call for the given key, potentially closing the circuit.
    fn record_success(&self, key: &str);

    /// Record a failed call for the given key, potentially opening the circuit.
    fn record_failure(&self, key: &str);
}

/// Bulkhead — concurrency limiter that caps the number of simultaneous
/// executions per key.
pub trait Bulkhead: Send + Sync {
    /// Acquire a concurrency permit for the given key.
    ///
    /// Returns `Ok(())` if a permit was acquired, or
    /// [`SchedulrError::BulkheadFull`] if the bulkhead is saturated.
    fn acquire(
        &self,
        key: &str,
        config: &BulkheadConfig,
    ) -> impl Future<Output = Result<(), SchedulrError>> + Send;

    /// Release a previously acquired concurrency permit for the given key.
    fn release(&self, key: &str);
}

/// Retry executor — retries a fallible async operation with configurable backoff.
pub trait RetryExecutor: Send + Sync {
    /// Execute the given operation, retrying on failure according to the config.
    ///
    /// The `operation` closure is called repeatedly until it succeeds or
    /// `max_attempts` is reached. Backoff between retries grows exponentially
    /// according to `backoff_multiplier`, capped at `max_delay`.
    fn execute_with_retry<F, Fut, T>(
        &self,
        operation: F,
        config: &RetryConfig,
    ) -> impl Future<Output = Result<T, SchedulrError>> + Send
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, SchedulrError>> + Send,
        T: Send;
}
