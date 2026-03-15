//! Resilience patterns: rate limiter, circuit breaker, bulkhead.

// Suppress unused-crate-dependencies for stub modules.
use parking_lot as _;
use tokio as _;
use tracing as _;

/// Token-bucket and sliding-window rate limiters.
pub mod rate_limiter;
/// Circuit breaker for failing downstream calls.
pub mod circuit_breaker;
/// Bulkhead pattern for concurrency isolation.
pub mod bulkhead;
