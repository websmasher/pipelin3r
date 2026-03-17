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
    /// Maximum number of keys tracked before stale entries are evicted.
    max_tracked_keys: usize,
}

impl Default for InMemoryCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCircuitBreaker {
    /// Create a new, empty circuit breaker with the default key limit (10,000).
    pub const fn new() -> Self {
        Self::with_max_keys(MAX_TRACKED_KEYS)
    }

    /// Create a new, empty circuit breaker with a custom maximum number of tracked keys.
    pub const fn with_max_keys(max: usize) -> Self {
        Self {
            state: RwLock::new(BTreeMap::new()),
            max_tracked_keys: max,
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

        // Ensure the current key exists before eviction so it is never
        // a victim.
        let needs_insert = !map.contains_key(key);
        if needs_insert {
            let _prev = map.insert(key.to_owned(), CircuitState::new());
        }

        // Evict when the map exceeds the size limit, but never evict the
        // current key and never drop circuits that are accumulating failures.
        if map.len() > self.max_tracked_keys {
            // First pass: remove closed circuits with empty history (truly idle).
            map.retain(|k, circuit| {
                k == key || circuit.state != State::Closed || !circuit.results.is_empty()
            });

            // Second pass if still over: remove closed circuits with the
            // fewest recorded results (least information loss).
            if map.len() > self.max_tracked_keys {
                let excess = map.len().saturating_sub(self.max_tracked_keys);
                let mut closed_with_history: Vec<_> = map
                    .iter()
                    .filter(|(k, c)| k.as_str() != key && c.state == State::Closed)
                    .map(|(k, c)| (k.clone(), c.results.len()))
                    .collect();
                // Evict those with fewest results first (least information loss).
                closed_with_history.sort_by_key(|(_, len)| *len);
                for (evict_key, _) in closed_with_history.into_iter().take(excess) {
                    let _removed = map.remove(&evict_key);
                }
            }
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

                // Don't evaluate failure rate until we have enough data.
                // A single failure out of 1 call = 100% and would
                // prematurely trip the circuit on a brand-new key.
                let min_calls = std::cmp::max(2, config.sliding_window_size / 2);
                let min_calls_usize = usize::try_from(min_calls).unwrap_or(2);
                if circuit.results.len() < min_calls_usize {
                    return Ok(());
                }

                let rate = circuit.failure_rate();
                if rate >= config.failure_rate_threshold {
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
mod tests;
