//! Trait definitions for resilience components.

use core::future::Future;

use crate::config::{BulkheadConfig, CircuitBreakerConfig, RateLimitConfig, RetryConfig};
use crate::error::Limit3rError;

/// Rate limiter — acquires permits per key within a configured time window.
pub trait RateLimiter: Send + Sync {
    /// Attempt to acquire a rate limit permit for the given key.
    ///
    /// Returns `Ok(())` if a permit was acquired, or
    /// [`Limit3rError::RateLimitExceeded`] if the limit is exhausted.
    fn acquire_permission(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> impl Future<Output = Result<(), Limit3rError>> + Send;
}

/// Circuit breaker — tracks failure rates per key and opens the circuit
/// when the failure threshold is exceeded.
pub trait CircuitBreaker: Send + Sync {
    /// Check whether the circuit is closed (requests permitted).
    ///
    /// # Errors
    ///
    /// Returns [`Limit3rError::CircuitOpen`] if the circuit is open.
    fn check_permitted(
        &self,
        key: &str,
        config: &CircuitBreakerConfig,
    ) -> Result<(), Limit3rError>;

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
    /// [`Limit3rError::BulkheadFull`] if the bulkhead is saturated.
    fn acquire(
        &self,
        key: &str,
        config: &BulkheadConfig,
    ) -> impl Future<Output = Result<(), Limit3rError>> + Send;

    /// Release a previously acquired concurrency permit for the given key.
    fn release(&self, key: &str);
}

/// Retry executor — retries a fallible async operation with configurable backoff.
///
/// The error type `E` must be convertible from [`Limit3rError`] so the executor
/// can produce [`Limit3rError::RetryExhausted`] in the caller's error type.
pub trait RetryExecutor: Send + Sync {
    /// Execute the given operation, retrying on failure according to the config.
    ///
    /// The `operation` closure is called repeatedly until it succeeds or
    /// `max_attempts` is reached. Backoff between retries grows exponentially
    /// according to `backoff_multiplier`, capped at `max_delay`.
    fn execute_with_retry<F, Fut, T, E>(
        &self,
        operation: F,
        config: &RetryConfig,
    ) -> impl Future<Output = Result<T, E>> + Send
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, E>> + Send,
        T: Send,
        E: From<Limit3rError> + Send;
}
