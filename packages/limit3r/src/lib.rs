//! Resilience patterns: rate limiter, circuit breaker, bulkhead, retry.
//!
//! This crate provides configurable, in-memory implementations of common
//! resilience patterns for async Rust services.

pub mod config;
pub mod duration_serde;
pub mod error;
pub mod traits;

mod bulkhead;
mod circuit_breaker;
mod jitter;
mod rate_limiter;
mod retry;

// Re-export config types at crate root for convenience.
pub use config::{BulkheadConfig, CircuitBreakerConfig, RateLimitConfig, RetryConfig};
pub use error::Limit3rError;

// Re-export trait definitions at crate root.
pub use traits::{Bulkhead, CircuitBreaker, RateLimiter, RetryExecutor};

// Re-export concrete implementations at crate root.
pub use bulkhead::InMemoryBulkhead;
pub use circuit_breaker::InMemoryCircuitBreaker;
pub use rate_limiter::InMemoryRateLimiter;
pub use retry::TokioRetryExecutor;
