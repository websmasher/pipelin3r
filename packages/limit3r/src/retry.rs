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
        E: From<Limit3rError> + Send,
    {
        let mut attempt: u32 = 0;
        loop {
            let result = operation().await;

            match result {
                Ok(value) => return Ok(value),
                Err(err) => {
                    attempt = attempt.saturating_add(1);
                    if attempt >= config.max_attempts {
                        tracing::warn!(
                            attempts = config.max_attempts,
                            "All retry attempts exhausted",
                        );
                        return Err(E::from(Limit3rError::RetryExhausted {
                            attempts: config.max_attempts,
                        }));
                    }
                    let delay = compute_delay(config, attempt);
                    tracing::debug!(
                        ?delay,
                        attempt,
                        max_attempts = config.max_attempts,
                        "Retrying after failure",
                    );
                    // Discard the intermediate error — we'll retry
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
                            Err(Limit3rError::RetryExhausted { attempts: 0 })
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
                || async { Err(Limit3rError::RetryExhausted { attempts: 0 }) },
                &config,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, Limit3rError::RetryExhausted { attempts: 2 }),
            "expected RetryExhausted with attempts=2, got {err:?}"
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
