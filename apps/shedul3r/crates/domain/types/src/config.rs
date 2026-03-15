//! Resilience configuration types for rate limiting, circuit breaking, retry, and bulkhead.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::duration_serde;

/// Rate limiter configuration: controls permits per time window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Number of permits available per refresh window.
    pub limit_for_period: u32,
    /// How often the permit counter resets.
    #[serde(with = "duration_serde")]
    pub limit_refresh_period: Duration,
    /// Maximum time to wait for a permit before failing.
    #[serde(with = "duration_serde")]
    pub timeout_duration: Duration,
}

/// Circuit breaker configuration: opens the circuit when failures exceed a threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure rate percentage that triggers the circuit to open (e.g. 50.0 = 50%).
    pub failure_rate_threshold: f64,
    /// Number of recent calls used to calculate the failure rate.
    pub sliding_window_size: u32,
    /// How long the circuit stays open before transitioning to half-open.
    #[serde(with = "duration_serde")]
    pub wait_duration_in_open_state: Duration,
}

/// Retry configuration: controls exponential backoff retry behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Total number of attempts including the first try.
    pub max_attempts: u32,
    /// Initial backoff duration before the first retry.
    #[serde(with = "duration_serde")]
    pub wait_duration: Duration,
    /// Multiplier applied to the backoff after each retry.
    pub backoff_multiplier: f64,
    /// Upper bound on the backoff duration.
    #[serde(with = "duration_serde")]
    pub max_delay: Duration,
}

/// Bulkhead configuration: limits concurrent execution per key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkheadConfig {
    /// Maximum number of concurrent permits.
    pub max_concurrent: u32,
    /// Maximum time to wait for a permit before failing.
    #[serde(with = "duration_serde")]
    pub max_wait_duration: Duration,
}
