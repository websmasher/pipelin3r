//! Count-based sliding window circuit breaker backed by in-memory state.

use std::collections::{BTreeMap, VecDeque};

use crate::config::CircuitBreakerConfig;
use crate::error::Limit3rError;
use crate::traits::CircuitBreaker;
use parking_lot::RwLock;
use tokio::time::Instant;

/// Possible states of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Normal operation — requests are allowed through.
    Closed,
    /// Circuit tripped — all requests are rejected.
    Open,
    /// Testing — one request is allowed to probe recovery.
    HalfOpen,
}

/// Per-key circuit breaker state.
#[derive(Debug)]
struct CircuitState {
    /// Current state of the circuit.
    state: State,
    /// Ring buffer of recent call results (`true` = success, `false` = failure).
    results: VecDeque<bool>,
    /// Timestamp when the circuit transitioned to `Open`.
    opened_at: Option<Instant>,
}

impl CircuitState {
    const fn new() -> Self {
        Self {
            state: State::Closed,
            results: VecDeque::new(),
            opened_at: None,
        }
    }

    /// Trim the results buffer to the given sliding window size.
    fn trim_to_window(&mut self, window_size: u32) {
        let max_len: usize = usize::try_from(window_size).unwrap_or(usize::MAX);
        while self.results.len() > max_len {
            let _discarded = self.results.pop_front();
        }
    }

    /// Calculate the failure rate as a percentage (0.0 to 100.0).
    fn failure_rate(&self) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }
        let failures = self.results.iter().filter(|r| !(**r)).count();
        let total_f64 = f64::from(u32::try_from(self.results.len()).unwrap_or(u32::MAX));
        let failures_f64 = f64::from(u32::try_from(failures).unwrap_or(u32::MAX));
        // Division is safe: total_f64 > 0 guaranteed by the early return
        #[allow(clippy::arithmetic_side_effects)] // f64 div of positive bounded counts
        let rate = failures_f64 / total_f64;
        #[allow(clippy::arithmetic_side_effects)] // f64 mul of [0,1] by 100.0
        let percentage = rate * 100.0;
        percentage
    }
}

/// Maximum number of keys tracked before stale entries are evicted.
const MAX_TRACKED_KEYS: usize = 10_000;

/// Type alias to reduce type complexity for the inner state map.
type StateMap = BTreeMap<String, CircuitState>;

/// In-memory count-based sliding window circuit breaker.
///
/// Tracks the last `sliding_window_size` call results per key. When the
/// failure rate exceeds `failure_rate_threshold` the circuit opens and
/// rejects all calls. After `wait_duration_in_open_state` it transitions
/// to half-open and allows a single probe call.
#[derive(Debug)]
pub struct InMemoryCircuitBreaker {
    state: RwLock<StateMap>,
}

impl Default for InMemoryCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCircuitBreaker {
    /// Create a new, empty circuit breaker.
    pub const fn new() -> Self {
        Self {
            state: RwLock::new(BTreeMap::new()),
        }
    }
}

impl CircuitBreaker for InMemoryCircuitBreaker {
    fn check_permitted(
        &self,
        key: &str,
        config: &CircuitBreakerConfig,
    ) -> Result<(), Limit3rError> {
        let mut map = self.state.write();

        // Evict idle closed circuits when the map exceeds the size limit.
        // All Closed keys are dropped — they can be re-created fresh on next access.
        // Open and HalfOpen keys are kept since they are actively tracking failures.
        if map.len() > MAX_TRACKED_KEYS {
            map.retain(|_k, circuit| circuit.state != State::Closed);
        }

        let needs_insert = !map.contains_key(key);
        if needs_insert {
            let _prev = map.insert(key.to_owned(), CircuitState::new());
        }
        let Some(circuit) = map.get_mut(key) else {
            return Err(Limit3rError::CircuitOpen {
                key: key.to_owned(),
            });
        };

        let result = match circuit.state {
            State::Closed => {
                // Evaluate failure rate to decide if we should open
                circuit.trim_to_window(config.sliding_window_size);
                let rate = circuit.failure_rate();
                if rate > config.failure_rate_threshold {
                    circuit.state = State::Open;
                    circuit.opened_at = Some(Instant::now());
                    tracing::info!(key, rate, "Circuit breaker opened");
                    Err(Limit3rError::CircuitOpen {
                        key: key.to_owned(),
                    })
                } else {
                    Ok(())
                }
            }
            State::Open => {
                // Check if wait duration has elapsed
                let should_transition = circuit.opened_at.is_some_and(|opened| {
                    Instant::now().duration_since(opened) >= config.wait_duration_in_open_state
                });

                if should_transition {
                    circuit.state = State::HalfOpen;
                    tracing::debug!(key, "Circuit breaker transitioning to half-open");
                    Ok(())
                } else {
                    Err(Limit3rError::CircuitOpen {
                        key: key.to_owned(),
                    })
                }
            }
            State::HalfOpen => {
                // Only one probe call allowed in half-open
                Err(Limit3rError::CircuitOpen {
                    key: key.to_owned(),
                })
            }
        };
        drop(map);
        result
    }

    fn record_success(&self, key: &str) {
        self.record_outcome(key, true);
    }

    fn record_failure(&self, key: &str) {
        self.record_outcome(key, false);
    }
}

impl InMemoryCircuitBreaker {
    /// Record a success or failure outcome for the given key.
    fn record_outcome(&self, key: &str, success: bool) {
        let mut map = self.state.write();
        let Some(circuit) = map.get_mut(key) else {
            return;
        };

        match circuit.state {
            State::HalfOpen => {
                if success {
                    circuit.state = State::Closed;
                    circuit.opened_at = None;
                    circuit.results.clear();
                    tracing::debug!(key, "Circuit breaker closed after successful probe");
                } else {
                    circuit.state = State::Open;
                    circuit.opened_at = Some(Instant::now());
                    tracing::debug!(key, "Circuit breaker reopened after failed probe");
                }
            }
            State::Closed => {
                circuit.results.push_back(success);
            }
            State::Open => {}
        }
        drop(map);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_config(threshold: f64, window: u32, wait: Duration) -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_rate_threshold: threshold,
            sliding_window_size: window,
            wait_duration_in_open_state: wait,
        }
    }

    #[test]
    fn starts_closed_and_permits_requests() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 5, Duration::from_secs(5));

        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn opens_after_failure_threshold_exceeded() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(5));

        // First call to initialize state
        cb.check_permitted("key-a", &config).unwrap();

        // Record 3 failures out of 4 window slots (75% > 50% threshold)
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");

        // Next check should trip the circuit
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_requests_when_open() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(60));

        // Initialize and force open
        cb.check_permitted("key-a", &config).unwrap();
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        let _ = cb.check_permitted("key-a", &config); // triggers open

        // Subsequent calls should be rejected
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn transitions_to_half_open_after_wait_duration() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_millis(50));

        // Initialize and force open
        cb.check_permitted("key-a", &config).unwrap();
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        let _ = cb.check_permitted("key-a", &config); // triggers open

        // Wait for the open duration to pass
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should now be allowed (half-open probe)
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn closes_again_after_successful_probe() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_millis(50));

        // Initialize and force open
        cb.check_permitted("key-a", &config).unwrap();
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        let _ = cb.check_permitted("key-a", &config); // triggers open

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(60)).await;
        cb.check_permitted("key-a", &config).unwrap(); // half-open probe

        // Record success to close the circuit
        cb.record_success("key-a");

        // Should now be fully closed
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn reopens_after_failed_probe() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_millis(50));

        // Initialize and force open
        cb.check_permitted("key-a", &config).unwrap();
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        let _ = cb.check_permitted("key-a", &config); // triggers open

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(60)).await;
        cb.check_permitted("key-a", &config).unwrap(); // half-open probe

        // Record failure to reopen
        cb.record_failure("key-a");

        // Should be open again (no wait elapsed)
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_err());
    }
}
