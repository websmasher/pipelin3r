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
    /// Jitter factor applied to the window-wait sleep (0.0 = none, 1.0 = ±100%).
    #[serde(default)]
    pub jitter_factor: f64,
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
    /// Jitter factor applied to the open-state wait duration (0.0 = none, 1.0 = ±100%).
    ///
    /// Sampled once when the circuit opens. If the circuit re-opens after a
    /// failed half-open probe, the same jittered value is reused until the
    /// circuit fully closes.
    #[serde(default)]
    pub jitter_factor: f64,
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
    /// Jitter factor applied to the backoff delay (0.0 = none, 1.0 = ±100%).
    #[serde(default)]
    pub jitter_factor: f64,
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

impl RateLimitConfig {
    /// Validate the configuration, returning a description of any problem found.
    ///
    /// # Errors
    ///
    /// Returns an error message if any field has an invalid value.
    pub fn validate(&self) -> Result<(), String> {
        if self.limit_for_period == 0 {
            return Err("limit_for_period must be greater than 0".to_owned());
        }
        if self.limit_refresh_period.is_zero() {
            return Err("limit_refresh_period must be greater than zero".to_owned());
        }
        if self.timeout_duration.is_zero() {
            return Err("timeout_duration must be greater than zero".to_owned());
        }
        if !self.jitter_factor.is_finite() || self.jitter_factor < 0.0 || self.jitter_factor > 1.0 {
            return Err(
                "jitter_factor must be a finite number between 0.0 and 1.0 inclusive".to_owned(),
            );
        }
        Ok(())
    }
}

impl CircuitBreakerConfig {
    /// Validate the configuration, returning a description of any problem found.
    ///
    /// # Errors
    ///
    /// Returns an error message if any field has an invalid value.
    pub fn validate(&self) -> Result<(), String> {
        if self.sliding_window_size == 0 {
            return Err("sliding_window_size must be greater than 0".to_owned());
        }
        if self.failure_rate_threshold < 0.0 || self.failure_rate_threshold > 100.0 {
            return Err(
                "failure_rate_threshold must be between 0.0 and 100.0 inclusive".to_owned(),
            );
        }
        if self.wait_duration_in_open_state.is_zero() {
            return Err("wait_duration_in_open_state must be greater than zero".to_owned());
        }
        if !self.jitter_factor.is_finite() || self.jitter_factor < 0.0 || self.jitter_factor > 1.0 {
            return Err(
                "jitter_factor must be a finite number between 0.0 and 1.0 inclusive".to_owned(),
            );
        }
        Ok(())
    }
}

impl RetryConfig {
    /// Validate the configuration, returning a description of any problem found.
    ///
    /// # Errors
    ///
    /// Returns an error message if any field has an invalid value.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_attempts == 0 {
            return Err("max_attempts must be greater than 0".to_owned());
        }
        if !self.backoff_multiplier.is_finite() || self.backoff_multiplier <= 0.0 {
            return Err("backoff_multiplier must be a positive finite number".to_owned());
        }
        if !self.jitter_factor.is_finite() || self.jitter_factor < 0.0 || self.jitter_factor > 1.0 {
            return Err(
                "jitter_factor must be a finite number between 0.0 and 1.0 inclusive".to_owned(),
            );
        }
        Ok(())
    }
}

impl BulkheadConfig {
    /// Validate the configuration, returning a description of any problem found.
    ///
    /// # Errors
    ///
    /// Returns an error message if `max_concurrent` is zero (no permits would
    /// ever be available).
    pub fn validate(&self) -> Result<(), String> {
        if self.max_concurrent == 0 {
            return Err("max_concurrent is 0 — no permits will ever be available".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
