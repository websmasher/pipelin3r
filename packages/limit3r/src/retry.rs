//! Retry executor with exponential backoff using tokio timers.

use std::time::Duration;

use crate::config::RetryConfig;
use crate::error::Limit3rError;
use crate::traits::RetryExecutor;

/// Retry executor that uses [`tokio::time::sleep`] between attempts.
///
/// Implements exponential backoff: the delay between retries grows by
/// `backoff_multiplier` each time, capped at `max_delay`. If all attempts
/// fail, returns [`Limit3rError::RetryExhausted`].
#[derive(Debug)]
pub struct TokioRetryExecutor;

impl Default for TokioRetryExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl TokioRetryExecutor {
    /// Create a new retry executor.
    pub const fn new() -> Self {
        Self
    }
}

/// Compute the backoff delay for a given attempt number.
///
/// Formula: `min(wait_duration * backoff_multiplier^attempt, max_delay)`
///
/// Uses `Duration::from_secs_f64` for the multiplication to avoid integer
/// overflow issues with checked arithmetic on `Duration`.
fn compute_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let base_secs = config.wait_duration.as_secs_f64();
    let multiplier = config.backoff_multiplier;

    // backoff_multiplier^attempt — powi takes i32, convert safely
    let exponent = i32::try_from(attempt).unwrap_or(i32::MAX);
    let factor = multiplier.powi(exponent);

    // base_secs * factor — both are f64, result may be very large
    #[allow(clippy::arithmetic_side_effects)] // f64 mul of Duration secs by bounded exponent
    let delay_secs = base_secs * factor;

    // Clamp to max_delay
    let max_secs = config.max_delay.as_secs_f64();
    let clamped = if delay_secs > max_secs {
        max_secs
    } else if delay_secs.is_nan() || delay_secs < 0.0 {
        0.0
    } else {
        delay_secs
    };

    Duration::from_secs_f64(clamped)
}

impl RetryExecutor for TokioRetryExecutor {
    async fn execute_with_retry<F, Fut, T, E>(
        &self,
        operation: F,
        config: &RetryConfig,
    ) -> Result<T, E>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, E>> + Send,
        T: Send,
        E: From<Limit3rError> + std::fmt::Display + Send,
    {
        let mut attempt: u32 = 0;
        loop {
            let result = operation().await;

            match result {
                Ok(value) => return Ok(value),
                Err(err) => {
                    let last_message = err.to_string();
                    attempt = attempt.saturating_add(1);
                    if attempt >= config.max_attempts {
                        tracing::warn!(
                            attempts = config.max_attempts,
                            %last_message,
                            "All retry attempts exhausted",
                        );
                        return Err(E::from(Limit3rError::RetryExhausted {
                            attempts: config.max_attempts,
                            last_message,
                        }));
                    }
                    let delay = compute_delay(config, attempt);
                    tracing::debug!(
                        ?delay,
                        attempt,
                        max_attempts = config.max_attempts,
                        "Retrying after failure",
                    );
                    drop(err);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn test_config(max_attempts: u32) -> RetryConfig {
        RetryConfig {
            max_attempts,
            wait_duration: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(1),
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
                            Err(Limit3rError::RetryExhausted { attempts: 0, last_message: String::new() })
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
                || async { Err(Limit3rError::RetryExhausted { attempts: 0, last_message: String::new() }) },
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
        // Mutant kill: retry.rs:50 — replace > with == or >=
        // When delay == max_delay, it should return max_delay (not be clamped differently).
        // With > correctly, delay == max is not > max, so it falls through to the else branch = delay_secs.
        // With >=, delay == max would enter the if branch and return max_secs — same result.
        // With ==, delay > max would NOT enter the if and would overflow.
        // So we test delay ABOVE max to kill == mutation.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(5),
        };
        // attempt 1: 10s * 2^1 = 20s, which is > 5s max_delay
        let d = compute_delay(&config, 1);
        assert_eq!(
            d,
            Duration::from_secs(5),
            "delay exceeding max_delay must be clamped to max_delay"
        );
    }

    #[test]
    fn mutant_kill_compute_delay_just_below_max_not_clamped() {
        // Mutant kill: retry.rs:50 — replace > with >=
        // delay < max should NOT be clamped. With >=, delay == max would be clamped (same value, so same result).
        // To distinguish > from >=: delay exactly at max should pass through unclamped.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_millis(500),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(1),
        };
        // attempt 1: 500ms * 2^1 = 1000ms = exactly max_delay
        let d = compute_delay(&config, 1);
        // Whether > or >=, result is the same when delay == max (both return max_secs).
        // The real kill is: delay clearly above max must clamp.
        assert_eq!(
            d,
            Duration::from_secs(1),
            "delay at max_delay boundary must return max_delay"
        );
    }

    #[test]
    fn mutant_kill_compute_delay_nan_returns_zero() {
        // Mutant kill: retry.rs:52 — replace || with && on NaN/negative check
        // NaN factor should produce zero delay, not propagate NaN.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: f64::NAN,
            max_delay: Duration::from_secs(5),
        };
        let d = compute_delay(&config, 1);
        assert_eq!(d, Duration::ZERO, "NaN backoff must produce zero delay");
    }

    #[test]
    fn mutant_kill_compute_delay_negative_factor_returns_zero() {
        // Mutant kill: retry.rs:52 — replace < with == or <=
        // A negative multiplier raised to odd power produces negative delay.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: -2.0,
            max_delay: Duration::from_secs(5),
        };
        // attempt 1: 100ms * (-2)^1 = -200ms — negative, should return zero
        let d = compute_delay(&config, 1);
        assert_eq!(
            d,
            Duration::ZERO,
            "negative delay must produce zero delay"
        );
    }

    #[test]
    fn mutant_kill_compute_delay_negative_not_equal_zero() {
        // Mutant kill: retry.rs:52 — replace < with ==
        // -0.2 is negative but not equal to 0.0, must still return zero.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_millis(100),
            backoff_multiplier: -1.0,
            max_delay: Duration::from_secs(5),
        };
        // attempt 1: 100ms * (-1)^1 = -100ms
        let d = compute_delay(&config, 1);
        assert_eq!(
            d,
            Duration::ZERO,
            "negative delay (not zero) must produce zero delay (< not ==)"
        );
    }

    #[test]
    fn mutant_kill_v2_delay_exactly_equal_max_returns_max() {
        // Mutant kill: retry.rs:50 — `> max_secs` replaced with `>= max_secs`
        // When delay_secs == max_secs exactly:
        //   With `>`:  delay is NOT > max, falls to else → returns delay_secs (== max_secs) ✓
        //   With `>=`: delay IS >= max, enters if → returns max_secs (same value) ✓
        // Both produce the same result when equal, so we can't distinguish.
        // Instead, test delay JUST BELOW max — it must NOT be clamped to max.
        // With `>=` and delay==max, it returns max (same). But if delay < max:
        //   With `>`:  not > max → returns delay_secs ✓
        //   With `>=`: not >= max → returns delay_secs ✓
        // Still same. The real kill: delay slightly above max must clamp.
        // Actually the `>` vs `>=` mutant can only be killed when delay == max exactly:
        // both return max_secs so it's equivalent. The REAL mutant to kill is `>` → `==`:
        // With `==`, only delay == max enters the if. delay > max skips and goes to
        // else → returns the huge delay_secs, which would cause Duration overflow.
        //
        // Test: delay clearly above max must be clamped.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_secs(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
        };
        // attempt 1: 100s * 2^1 = 200s, clearly > 10s
        let d = compute_delay(&config, 1);
        assert_eq!(
            d,
            Duration::from_secs(10),
            "delay above max must be clamped to max_delay"
        );

        // Also test: delay exactly at max returns max (not zero or something else)
        let config_exact = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_millis(500),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(1),
        };
        // attempt 1: 500ms * 2 = 1000ms = exactly max_delay
        let d_exact = compute_delay(&config_exact, 1);
        assert_eq!(
            d_exact,
            Duration::from_secs(1),
            "delay exactly at max must return max"
        );
    }

    #[test]
    fn mutant_kill_v2_delay_exactly_zero_not_rejected() {
        // Mutant kill: retry.rs:52 — `< 0.0` replaced with `<= 0.0`
        // With `<`: 0.0 is NOT < 0.0, so it falls through to else → returns 0.0 ✓
        // With `<=`: 0.0 IS <= 0.0, enters the if → returns 0.0 ✓
        // Same result! Both return 0.0. The real kill: a tiny positive value (like 0.001)
        // must NOT be treated as negative.
        //   With `<`: 0.001 not < 0 → else → returns 0.001 ✓
        //   With `<=`: 0.001 not <= 0 → else → returns 0.001 ✓
        // Still same. Actually for `<` vs `<=`, the boundary IS 0.0:
        // value = 0.0: `<` → false (returns 0.0 via else), `<=` → true (returns 0.0 via if)
        // Both return Duration::from_secs_f64(0.0) = Duration::ZERO. Semantically identical.
        //
        // The real mutant to kill: `<` replaced with `==`. With `==`:
        //   -0.5: not == 0.0 → else → returns -0.5 → Duration::from_secs_f64(-0.5) PANICS!
        // That's already caught by existing negative tests. But let's verify 0.0 explicitly.
        let config = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_secs(0),
            backoff_multiplier: 1.0,
            max_delay: Duration::from_secs(5),
        };
        // attempt 0: 0s * 1^0 = 0.0
        let d = compute_delay(&config, 0);
        assert_eq!(
            d,
            Duration::ZERO,
            "zero delay must produce Duration::ZERO, not be rejected"
        );

        // Also: a small positive delay must NOT be clamped to zero
        let config_small = RetryConfig {
            max_attempts: 5,
            wait_duration: Duration::from_millis(1),
            backoff_multiplier: 1.0,
            max_delay: Duration::from_secs(5),
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
        };

        // attempt 0: 100ms * 2^0 = 100ms
        let d0 = compute_delay(&config, 0);
        assert_eq!(d0, Duration::from_millis(100));

        // attempt 1: 100ms * 2^1 = 200ms
        let d1 = compute_delay(&config, 1);
        assert_eq!(d1, Duration::from_millis(200));

        // attempt 2: 100ms * 2^2 = 400ms
        let d2 = compute_delay(&config, 2);
        assert_eq!(d2, Duration::from_millis(400));
    }
}
