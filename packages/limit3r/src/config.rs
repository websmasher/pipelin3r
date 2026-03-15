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
            return Err(
                "max_concurrent is 0 — no permits will ever be available".to_owned(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use super::*;

    #[test]
    fn rate_limit_config_serde_round_trip() {
        let config = RateLimitConfig {
            limit_for_period: 100,
            limit_refresh_period: Duration::from_secs(60),
            timeout_duration: Duration::from_millis(500),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RateLimitConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.limit_for_period, config.limit_for_period);
        assert_eq!(
            deserialized.limit_refresh_period,
            config.limit_refresh_period
        );
        assert_eq!(deserialized.timeout_duration, config.timeout_duration);
    }

    #[test]
    fn circuit_breaker_config_serde_round_trip() {
        let config = CircuitBreakerConfig {
            failure_rate_threshold: 50.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::from_secs(30),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CircuitBreakerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.failure_rate_threshold,
            config.failure_rate_threshold
        );
        assert_eq!(
            deserialized.sliding_window_size,
            config.sliding_window_size
        );
        assert_eq!(
            deserialized.wait_duration_in_open_state,
            config.wait_duration_in_open_state
        );
    }

    #[test]
    fn retry_config_serde_round_trip() {
        let config = RetryConfig {
            max_attempts: 3,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_attempts, config.max_attempts);
        assert_eq!(deserialized.wait_duration, config.wait_duration);
        assert_eq!(deserialized.backoff_multiplier, config.backoff_multiplier);
        assert_eq!(deserialized.max_delay, config.max_delay);
    }

    // --- Config validation tests ---

    #[test]
    fn rate_limit_config_rejects_zero_limit_for_period() {
        let config = RateLimitConfig {
            limit_for_period: 0,
            limit_refresh_period: Duration::from_secs(1),
            timeout_duration: Duration::from_secs(1),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rate_limit_config_rejects_zero_refresh_period() {
        let config = RateLimitConfig {
            limit_for_period: 10,
            limit_refresh_period: Duration::ZERO,
            timeout_duration: Duration::from_secs(1),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rate_limit_config_rejects_zero_timeout() {
        let config = RateLimitConfig {
            limit_for_period: 10,
            limit_refresh_period: Duration::from_secs(1),
            timeout_duration: Duration::ZERO,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rate_limit_config_accepts_valid() {
        let config = RateLimitConfig {
            limit_for_period: 10,
            limit_refresh_period: Duration::from_secs(1),
            timeout_duration: Duration::from_secs(1),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn circuit_breaker_config_rejects_zero_window() {
        let config = CircuitBreakerConfig {
            failure_rate_threshold: 50.0,
            sliding_window_size: 0,
            wait_duration_in_open_state: Duration::from_secs(5),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn circuit_breaker_config_rejects_negative_threshold() {
        let config = CircuitBreakerConfig {
            failure_rate_threshold: -1.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::from_secs(5),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn circuit_breaker_config_rejects_threshold_over_100() {
        let config = CircuitBreakerConfig {
            failure_rate_threshold: 101.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::from_secs(5),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn circuit_breaker_config_rejects_zero_wait_duration() {
        let config = CircuitBreakerConfig {
            failure_rate_threshold: 50.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::ZERO,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn circuit_breaker_config_accepts_valid() {
        let config = CircuitBreakerConfig {
            failure_rate_threshold: 50.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::from_secs(5),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn circuit_breaker_config_accepts_boundary_thresholds() {
        let zero = CircuitBreakerConfig {
            failure_rate_threshold: 0.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::from_secs(5),
        };
        assert!(zero.validate().is_ok());

        let hundred = CircuitBreakerConfig {
            failure_rate_threshold: 100.0,
            sliding_window_size: 10,
            wait_duration_in_open_state: Duration::from_secs(5),
        };
        assert!(hundred.validate().is_ok());
    }

    #[test]
    fn retry_config_rejects_zero_attempts() {
        let config = RetryConfig {
            max_attempts: 0,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn retry_config_rejects_nan_multiplier() {
        let config = RetryConfig {
            max_attempts: 3,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: f64::NAN,
            max_delay: Duration::from_secs(10),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn retry_config_rejects_infinite_multiplier() {
        let config = RetryConfig {
            max_attempts: 3,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: f64::INFINITY,
            max_delay: Duration::from_secs(10),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn retry_config_rejects_negative_multiplier() {
        let config = RetryConfig {
            max_attempts: 3,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: -1.0,
            max_delay: Duration::from_secs(10),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn retry_config_rejects_zero_multiplier() {
        let config = RetryConfig {
            max_attempts: 3,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: 0.0,
            max_delay: Duration::from_secs(10),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn retry_config_accepts_valid() {
        let config = RetryConfig {
            max_attempts: 3,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn bulkhead_config_rejects_zero_max_concurrent() {
        let config = BulkheadConfig {
            max_concurrent: 0,
            max_wait_duration: Duration::from_millis(100),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn bulkhead_config_accepts_valid() {
        let config = BulkheadConfig {
            max_concurrent: 5,
            max_wait_duration: Duration::from_millis(100),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn bulkhead_config_serde_round_trip() {
        let config = BulkheadConfig {
            max_concurrent: 5,
            max_wait_duration: Duration::from_millis(200),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BulkheadConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_concurrent, config.max_concurrent);
        assert_eq!(deserialized.max_wait_duration, config.max_wait_duration);
    }
}
