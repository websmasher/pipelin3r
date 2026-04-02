#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]

use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

fn test_config(max_attempts: u32) -> RetryConfig {
    RetryConfig {
        max_attempts,
        wait_duration: Duration::from_millis(10),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(1),
        jitter_factor: 0.0,
    }
}

#[tokio::test]
async fn succeeds_on_first_attempt_no_retry() {
    let executor = TokioRetryExecutor::new();
    let config = test_config(3);
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = Arc::clone(&call_count);

    let result: Result<&str, Limit3rError> = executor
        .execute_with_retry(
            move || {
                let cc = Arc::clone(&cc);
                async move {
                    let _prev = cc.fetch_add(1, Ordering::SeqCst);
                    Ok("ok")
                }
            },
            &config,
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn succeeds_after_retry() {
    let executor = TokioRetryExecutor::new();
    let config = test_config(3);
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = Arc::clone(&call_count);

    let result: Result<&str, Limit3rError> = executor
        .execute_with_retry(
            move || {
                let cc = Arc::clone(&cc);
                async move {
                    let attempt = cc.fetch_add(1, Ordering::SeqCst);
                    if attempt < 1 {
                        Err(Limit3rError::RetryExhausted {
                            attempts: 0,
                            last_message: String::new(),
                        })
                    } else {
                        Ok("ok")
                    }
                }
            },
            &config,
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn exhausts_all_attempts_and_returns_retry_exhausted() {
    let executor = TokioRetryExecutor::new();
    let config = test_config(2);

    let result: Result<&str, Limit3rError> = executor
        .execute_with_retry(
            || async {
                Err(Limit3rError::RetryExhausted {
                    attempts: 0,
                    last_message: String::new(),
                })
            },
            &config,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Limit3rError::RetryExhausted { attempts: 2, .. }),
        "expected RetryExhausted with attempts=2, got {err:?}"
    );
}

#[test]
fn mutant_kill_compute_delay_exactly_at_max_returns_max() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_secs(10),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(
        d,
        Duration::from_secs(5),
        "delay exceeding max_delay must be clamped to max_delay"
    );
}

#[test]
fn mutant_kill_compute_delay_just_below_max_not_clamped() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(500),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(1),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(
        d,
        Duration::from_secs(1),
        "delay at max_delay boundary must return max_delay"
    );
}

#[test]
fn mutant_kill_compute_delay_nan_returns_zero() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: f64::NAN,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(d, Duration::ZERO, "NaN backoff must produce zero delay");
}

#[test]
fn mutant_kill_compute_delay_negative_factor_returns_zero() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: -2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(d, Duration::ZERO, "negative delay must produce zero delay");
}

#[test]
fn mutant_kill_compute_delay_negative_not_equal_zero() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: -1.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(
        d,
        Duration::ZERO,
        "negative delay (not zero) must produce zero delay (< not ==)"
    );
}

#[test]
fn mutant_kill_v2_delay_exactly_equal_max_returns_max() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_secs(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(
        d,
        Duration::from_secs(10),
        "delay above max must be clamped to max_delay"
    );

    let config_exact = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(500),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(1),
        jitter_factor: 0.0,
    };
    let d_exact = compute_delay(&config_exact, 1);
    assert_eq!(
        d_exact,
        Duration::from_secs(1),
        "delay exactly at max must return max"
    );
}

#[test]
fn mutant_kill_v2_delay_exactly_zero_not_rejected() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_secs(0),
        backoff_multiplier: 1.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    let d = compute_delay(&config, 0);
    assert_eq!(
        d,
        Duration::ZERO,
        "zero delay must produce Duration::ZERO, not be rejected"
    );

    let config_small = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(1),
        backoff_multiplier: 1.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    let d_small = compute_delay(&config_small, 0);
    assert_eq!(
        d_small,
        Duration::from_millis(1),
        "small positive delay must not be clamped to zero"
    );
}

#[test]
fn compute_delay_with_exponential_backoff() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };

    let d0 = compute_delay(&config, 0);
    assert_eq!(d0, Duration::from_millis(100));

    let d1 = compute_delay(&config, 1);
    assert_eq!(d1, Duration::from_millis(200));

    let d2 = compute_delay(&config, 2);
    assert_eq!(d2, Duration::from_millis(400));
}

// --- Jitter tests ---

#[test]
fn compute_delay_with_jitter_produces_variable_output() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_secs(1),
        backoff_multiplier: 1.0,
        max_delay: Duration::from_secs(10),
        jitter_factor: 0.5,
    };
    // With factor=0.5, delay for attempt 0 = 1s, jittered to [0.5s, 1.5s]
    let mut results = std::collections::BTreeSet::new();
    for _ in 0..50 {
        let d = compute_delay(&config, 0);
        assert!(
            d >= Duration::from_millis(500),
            "jittered delay {d:?} below lower bound 500ms"
        );
        assert!(
            d <= Duration::from_millis(1500),
            "jittered delay {d:?} above upper bound 1500ms"
        );
        let _new = results.insert(d.as_millis());
    }
    assert!(
        results.len() > 5,
        "expected variable delays with jitter, got only {} distinct values",
        results.len()
    );
}

#[test]
fn compute_delay_zero_jitter_is_deterministic() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.0,
    };
    // With no jitter, repeated calls must return the exact same value.
    let d1 = compute_delay(&config, 1);
    let d2 = compute_delay(&config, 1);
    assert_eq!(d1, d2, "zero jitter must produce deterministic delays");
    assert_eq!(d1, Duration::from_millis(200));
}

#[test]
fn compute_delay_nan_backoff_with_nonzero_jitter_returns_zero() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: f64::NAN,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.5,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(
        d,
        Duration::ZERO,
        "NaN backoff with nonzero jitter must produce zero, not panic"
    );
}

#[test]
fn compute_delay_jitter_never_exceeds_max_delay() {
    // max_delay is a hard ceiling: jitter must not push the delay above it.
    let config = RetryConfig {
        max_attempts: 10,
        wait_duration: Duration::from_secs(100), // far above max
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 1.0,
    };
    for _ in 0..200 {
        let d = compute_delay(&config, 1);
        assert!(
            d <= Duration::from_secs(5),
            "jittered delay {d:?} exceeds max_delay 5s"
        );
    }
}

#[test]
fn compute_delay_jitter_on_clamped_delay_within_bounds() {
    // When backoff saturates to max_delay, jitter widens the range but
    // the result is still clamped to max_delay.
    let config = RetryConfig {
        max_attempts: 10,
        wait_duration: Duration::from_secs(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.5,
    };
    for _ in 0..200 {
        let d = compute_delay(&config, 1);
        // Jitter range on 5s with factor=0.5: [2.5s, 7.5s], clamped to 5s.
        // So effective range is [2.5s, 5s].
        assert!(
            d >= Duration::from_millis(2500),
            "jittered clamped delay {d:?} below 2.5s lower bound"
        );
        assert!(
            d <= Duration::from_secs(5),
            "jittered clamped delay {d:?} above max_delay 5s"
        );
    }
}

#[test]
fn compute_delay_extreme_attempt_clamped_and_jittered() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.5,
    };
    // u32::MAX causes powi overflow → +inf → clamped to max_delay → jittered
    let d = compute_delay(&config, u32::MAX);
    assert!(
        d <= Duration::from_secs(5),
        "extreme attempt delay {d:?} exceeds max_delay"
    );
    assert!(
        d >= Duration::from_millis(2500),
        "extreme attempt delay {d:?} below jitter lower bound 2.5s"
    );
}

#[test]
fn compute_delay_zero_wait_with_nonzero_jitter_returns_zero() {
    let config = RetryConfig {
        max_attempts: 5,
        wait_duration: Duration::ZERO,
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(5),
        jitter_factor: 0.5,
    };
    let d = compute_delay(&config, 1);
    assert_eq!(
        d,
        Duration::ZERO,
        "zero wait_duration with nonzero jitter must produce zero delay"
    );
}

#[tokio::test]
async fn max_attempts_one_no_retry_no_sleep() {
    let executor = TokioRetryExecutor::new();
    let config = RetryConfig {
        max_attempts: 1,
        wait_duration: Duration::from_secs(10),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(60),
        jitter_factor: 1.0,
    };
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = Arc::clone(&call_count);

    let start = std::time::Instant::now();
    let result: Result<&str, Limit3rError> = executor
        .execute_with_retry(
            move || {
                let cc = Arc::clone(&cc);
                async move {
                    let _prev = cc.fetch_add(1, Ordering::SeqCst);
                    Err(Limit3rError::RetryExhausted {
                        attempts: 0,
                        last_message: String::new(),
                    })
                }
            },
            &config,
        )
        .await;

    assert!(result.is_err());
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "max_attempts=1 must call operation exactly once"
    );
    // Should complete near-instantly (no sleep)
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "max_attempts=1 should not sleep"
    );
}
