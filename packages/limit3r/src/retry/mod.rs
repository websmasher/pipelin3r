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
mod tests;
