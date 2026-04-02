#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: deserializing test fixtures"
)]

use super::*;

#[test]
fn rate_limit_config_serde_round_trip() {
    let config = RateLimitConfig {
        limit_for_period: 100,
        limit_refresh_period: Duration::from_secs(60),
        timeout_duration: Duration::from_millis(500),
        jitter_factor: 0.0,
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
#[allow(clippy::float_cmp, reason = "test assertion: exact value expected")]
fn circuit_breaker_config_serde_round_trip() {
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(30),
        jitter_factor: 0.0,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: CircuitBreakerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.failure_rate_threshold,
        config.failure_rate_threshold
    );
    assert_eq!(deserialized.sliding_window_size, config.sliding_window_size);
    assert_eq!(
        deserialized.wait_duration_in_open_state,
        config.wait_duration_in_open_state
    );
}

#[test]
#[allow(clippy::float_cmp, reason = "test assertion: exact value expected")]
fn retry_config_serde_round_trip() {
    let config = RetryConfig {
        max_attempts: 3,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn rate_limit_config_rejects_zero_refresh_period() {
    let config = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::ZERO,
        timeout_duration: Duration::from_secs(1),
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn rate_limit_config_rejects_zero_timeout() {
    let config = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::ZERO,
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn rate_limit_config_accepts_valid() {
    let config = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_ok());
}

#[test]
fn circuit_breaker_config_rejects_zero_window() {
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 0,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn circuit_breaker_config_rejects_negative_threshold() {
    let config = CircuitBreakerConfig {
        failure_rate_threshold: -1.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn circuit_breaker_config_rejects_threshold_over_100() {
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 101.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn circuit_breaker_config_rejects_zero_wait_duration() {
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::ZERO,
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn circuit_breaker_config_accepts_valid() {
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    assert!(config.validate().is_ok());
}

#[test]
fn circuit_breaker_config_accepts_boundary_thresholds() {
    let zero = CircuitBreakerConfig {
        failure_rate_threshold: 0.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    assert!(zero.validate().is_ok());

    let hundred = CircuitBreakerConfig {
        failure_rate_threshold: 100.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
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
        jitter_factor: 0.0,
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
fn regression_config_validation_rejects_invalid_values() {
    let rl = RateLimitConfig {
        limit_for_period: 0,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: 0.0,
    };
    assert!(rl.validate().is_err(), "limit_for_period=0 must fail");

    let cb = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 0,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    assert!(cb.validate().is_err(), "sliding_window_size=0 must fail");

    let retry = RetryConfig {
        max_attempts: 0,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: 0.0,
    };
    assert!(retry.validate().is_err(), "max_attempts=0 must fail");

    let retry_inf = RetryConfig {
        max_attempts: 3,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: f64::INFINITY,
        max_delay: Duration::from_secs(10),
        jitter_factor: 0.0,
    };
    assert!(
        retry_inf.validate().is_err(),
        "backoff_multiplier=INFINITY must fail"
    );
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

// --- Jitter factor validation tests ---

#[test]
fn jitter_factor_rejects_negative() {
    let rl = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: -0.1,
    };
    assert!(rl.validate().is_err(), "negative jitter_factor must fail");

    let cb = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: -0.5,
    };
    assert!(cb.validate().is_err(), "negative jitter_factor must fail");

    let retry = RetryConfig {
        max_attempts: 3,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: -1.0,
    };
    assert!(
        retry.validate().is_err(),
        "negative jitter_factor must fail"
    );
}

#[test]
fn jitter_factor_rejects_over_one() {
    let rl = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: 1.1,
    };
    assert!(rl.validate().is_err(), "jitter_factor > 1.0 must fail");

    let cb = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 2.0,
    };
    assert!(cb.validate().is_err(), "jitter_factor > 1.0 must fail");

    let retry = RetryConfig {
        max_attempts: 3,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: 1.5,
    };
    assert!(retry.validate().is_err(), "jitter_factor > 1.0 must fail");
}

#[test]
fn jitter_factor_accepts_boundaries() {
    let zero = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: 0.0,
    };
    assert!(zero.validate().is_ok(), "jitter_factor=0.0 must be valid");

    let one = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: 1.0,
    };
    assert!(one.validate().is_ok(), "jitter_factor=1.0 must be valid");

    let mid = RetryConfig {
        max_attempts: 3,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: 0.5,
    };
    assert!(mid.validate().is_ok(), "jitter_factor=0.5 must be valid");
}

#[test]
fn jitter_factor_serde_default_omitted() {
    // Configs serialized without jitter_factor should deserialize with 0.0
    // duration_serde uses f64 fractional seconds
    let json = r#"{"limit_for_period":10,"limit_refresh_period":1.0,"timeout_duration":1.0}"#;
    let config: RateLimitConfig = serde_json::from_str(json).unwrap();
    #[allow(clippy::float_cmp, reason = "test: verifying exact serde default")]
    {
        assert_eq!(config.jitter_factor, 0.0);
    }
}

#[test]
fn jitter_factor_rejects_nan() {
    let rl = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: f64::NAN,
    };
    assert!(rl.validate().is_err(), "NaN jitter_factor must fail");

    let cb = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 10,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: f64::NAN,
    };
    assert!(cb.validate().is_err(), "NaN jitter_factor must fail");

    let retry = RetryConfig {
        max_attempts: 3,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: f64::NAN,
    };
    assert!(retry.validate().is_err(), "NaN jitter_factor must fail");
}

#[test]
fn jitter_factor_rejects_infinity() {
    let rl = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: f64::INFINITY,
    };
    assert!(rl.validate().is_err(), "INFINITY jitter_factor must fail");

    let neg = RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(1),
        timeout_duration: Duration::from_secs(1),
        jitter_factor: f64::NEG_INFINITY,
    };
    assert!(
        neg.validate().is_err(),
        "NEG_INFINITY jitter_factor must fail"
    );
}
