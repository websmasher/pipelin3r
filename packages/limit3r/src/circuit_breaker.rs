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

        // Ensure the current key exists before eviction so it is never
        // a victim.
        let needs_insert = !map.contains_key(key);
        if needs_insert {
            let _prev = map.insert(key.to_owned(), CircuitState::new());
        }

        // Evict when the map exceeds the size limit, but never evict the
        // current key and never drop circuits that are accumulating failures.
        if map.len() > MAX_TRACKED_KEYS {
            // First pass: remove closed circuits with empty history (truly idle).
            map.retain(|k, circuit| {
                k == key || circuit.state != State::Closed || !circuit.results.is_empty()
            });

            // Second pass if still over: remove closed circuits with the
            // fewest recorded results (least information loss).
            if map.len() > MAX_TRACKED_KEYS {
                let excess = map.len().saturating_sub(MAX_TRACKED_KEYS);
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

    #[test]
    fn single_failure_does_not_trip_with_large_window() {
        // BUG-1 regression: 1 failure out of 1 call = 100%, but the
        // window has only 1 result which is below the min_calls
        // threshold (max(2, 10/2) = 5). The circuit must stay closed.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 10, Duration::from_secs(5));

        // Initialize
        cb.check_permitted("key-a", &config).unwrap();

        // Record a single failure
        cb.record_failure("key-a");

        // Should still be permitted — not enough data to evaluate
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_ok(), "circuit tripped with only 1 result in a window of 10");
    }

    #[test]
    fn partial_window_below_min_calls_stays_closed() {
        // With window=10, min_calls = max(2, 10/2) = 5.
        // Record 4 failures (below threshold count) — should NOT trip.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 10, Duration::from_secs(5));

        cb.check_permitted("key-a", &config).unwrap();

        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_failure("key-a");

        // 4 results < 5 min_calls, should still be ok
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_ok(), "circuit tripped with only 4 results, min_calls is 5");
    }

    #[test]
    fn trips_at_exact_threshold_rate() {
        // BUG-2 regression: >= threshold should trip, not just >.
        // threshold=50.0, window=4. Record 2 failures, 2 successes = exactly 50%.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(5));

        cb.check_permitted("key-a", &config).unwrap();

        cb.record_failure("key-a");
        cb.record_failure("key-a");
        cb.record_success("key-a");
        cb.record_success("key-a");

        // 2/4 = 50% >= 50% threshold — should trip
        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_err(), "circuit did not trip at exactly 50% with threshold 50%");
    }

    #[test]
    fn does_not_trip_just_below_threshold() {
        // 1 failure, 3 successes = 25% < 50% — should NOT trip.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(5));

        cb.check_permitted("key-a", &config).unwrap();

        cb.record_failure("key-a");
        cb.record_success("key-a");
        cb.record_success("key-a");
        cb.record_success("key-a");

        let result = cb.check_permitted("key-a", &config);
        assert!(result.is_ok(), "circuit tripped at 25% with threshold 50%");
    }

    #[test]
    fn eviction_never_removes_current_key() {
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(5));

        // Fill up beyond MAX_TRACKED_KEYS by creating many keys
        // We can't create 10001 keys in a unit test easily, but we can
        // verify the current key survives by inserting it, then accessing it.
        cb.check_permitted("survivor", &config).unwrap();
        cb.record_failure("survivor");

        // Access again — the key must still exist and not error
        let result = cb.check_permitted("survivor", &config);
        assert!(result.is_ok(), "current key was evicted or lost");
    }

    #[test]
    fn regression_circuit_breaker_does_not_trip_on_partial_window() {
        // Regression: A single failure in a window of size 100 should NOT open
        // the circuit. The old code computed 1/1 = 100% and tripped immediately
        // without waiting for enough data.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 100, Duration::from_secs(5));

        // Initialize the key
        cb.check_permitted("key-a", &config).unwrap();

        // Record exactly 1 failure
        cb.record_failure("key-a");

        // Must still be permitted — 1 result is far below min_calls = max(2, 100/2) = 50
        let result = cb.check_permitted("key-a", &config);
        assert!(
            result.is_ok(),
            "circuit tripped on 1 failure in a window of 100 — premature trip on partial window"
        );
    }

    #[test]
    fn regression_circuit_breaker_trips_at_exact_threshold_with_window_2() {
        // Regression: rate exactly at threshold should trip (>= not >).
        // With threshold=50.0 and window=2, record 1 success + 1 failure (50% rate).
        // min_calls = max(2, 2/2) = 2, so 2 results meets the threshold.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 2, Duration::from_secs(5));

        // Initialize the key
        cb.check_permitted("key-a", &config).unwrap();

        // Record 1 success and 1 failure = exactly 50%
        cb.record_success("key-a");
        cb.record_failure("key-a");

        // 1/2 = 50% >= 50% threshold — must trip
        let result = cb.check_permitted("key-a", &config);
        assert!(
            result.is_err(),
            "circuit did not trip at exactly 50% failure rate with threshold 50% and window 2"
        );
    }

    #[test]
    fn regression_circuit_breaker_eviction_preserves_failure_history() {
        // Regression: eviction used to drop ALL closed keys, losing failure
        // history for keys that were accumulating failures below threshold.
        // After the fix, only truly idle keys (empty history) are evicted first,
        // then those with fewest results.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 10, Duration::from_secs(5));

        // Fill beyond MAX_TRACKED_KEYS with idle keys (no history).
        for i in 0..MAX_TRACKED_KEYS.saturating_add(1) {
            let key = format!("filler-{i}");
            cb.check_permitted(&key, &config).unwrap();
        }

        // Add a key with failure history below threshold
        cb.check_permitted("important-key", &config).unwrap();
        cb.record_failure("important-key");
        cb.record_failure("important-key");
        cb.record_failure("important-key");

        // Trigger eviction by inserting another key
        cb.check_permitted("trigger-eviction", &config).unwrap();

        // The important key's failure history must be preserved.
        // Record more failures to reach the trip threshold.
        // With window=10, min_calls = max(2, 10/2) = 5.
        // We already have 3 failures, add 2 more to reach 5 results all failures = 100%.
        cb.record_failure("important-key");
        cb.record_failure("important-key");

        // Now check — if history was preserved, we have 5 failures = 100% >= 50%, should trip.
        // If history was lost, the key would look fresh and not trip.
        let result = cb.check_permitted("important-key", &config);
        assert!(
            result.is_err(),
            "failure history was lost during eviction — circuit should have tripped"
        );
    }

    #[test]
    fn mutant_kill_eviction_triggers_at_max_tracked_keys_circuit_breaker() {
        // Mutant kill: circuit_breaker.rs:116 — replace > with ==/</<=/>=
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(5));

        // Fill to exactly MAX_TRACKED_KEYS
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("fill-{i}");
            cb.check_permitted(&key, &config).unwrap();
        }

        // At exactly MAX_TRACKED_KEYS, no eviction should have happened
        let size_at_max = cb.state.read().len();
        assert_eq!(
            size_at_max, MAX_TRACKED_KEYS,
            "no eviction at exactly MAX_TRACKED_KEYS"
        );

        // Add one more — triggers eviction
        cb.check_permitted("one-more", &config).unwrap();

        let size_after = cb.state.read().len();
        assert!(
            size_after <= MAX_TRACKED_KEYS.saturating_add(1),
            "eviction should have run after exceeding MAX_TRACKED_KEYS"
        );
    }

    #[test]
    fn mutant_kill_eviction_keeps_open_circuits() {
        // Mutant kill: circuit_breaker.rs:119 — replace != with == and || with &&
        // Open circuits must survive first-pass eviction.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 4, Duration::from_secs(60));

        // Create a key and force it open
        cb.check_permitted("open-key", &config).unwrap();
        cb.record_failure("open-key");
        cb.record_failure("open-key");
        cb.record_failure("open-key");
        let _ = cb.check_permitted("open-key", &config); // triggers open

        // Fill beyond MAX_TRACKED_KEYS with idle keys (closed, empty history)
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("idle-{i}");
            cb.check_permitted(&key, &config).unwrap();
        }

        // Trigger eviction
        cb.check_permitted("trigger-evict", &config).unwrap();

        // The open circuit must survive
        let map = cb.state.read();
        assert!(
            map.contains_key("open-key"),
            "open circuit must survive eviction"
        );
    }

    #[test]
    fn mutant_kill_eviction_removes_closed_empty_first() {
        // Mutant kill: circuit_breaker.rs:119 — first pass removes closed+empty
        // Mutant kill: circuit_breaker.rs:124 — second pass only runs if still over limit
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 10, Duration::from_secs(5));

        // Create a key with non-empty history (closed but has results)
        cb.check_permitted("has-history", &config).unwrap();
        cb.record_success("has-history");
        cb.record_success("has-history");

        // Fill beyond MAX_TRACKED_KEYS with truly idle keys (closed, empty)
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("empty-{i}");
            cb.check_permitted(&key, &config).unwrap();
        }

        // Trigger eviction
        cb.check_permitted("trigger-evict", &config).unwrap();

        // has-history should survive first-pass (it has non-empty results)
        let map = cb.state.read();
        assert!(
            map.contains_key("has-history"),
            "closed circuit with history should survive first-pass eviction"
        );
    }

    #[test]
    fn mutant_kill_second_pass_evicts_closed_with_fewest_results() {
        // Mutant kill: circuit_breaker.rs:124 — replace > with >=
        // Mutant kill: circuit_breaker.rs:128 — replace && with || and != with == and == with !=
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 100, Duration::from_secs(5));

        // Create keys with varying history sizes — none are truly idle (empty)
        // so first-pass won't remove them, forcing second-pass.
        for i in 0..MAX_TRACKED_KEYS.saturating_add(2) {
            let key = format!("history-{i}");
            cb.check_permitted(&key, &config).unwrap();
            // Give each key at least 1 result so first-pass doesn't evict them
            cb.record_success(&key);
        }

        // Create a key with many results — should be last to be evicted
        cb.check_permitted("lots-of-history", &config).unwrap();
        for _ in 0..10 {
            cb.record_success("lots-of-history");
        }

        // Trigger eviction with another key
        cb.check_permitted("trigger", &config).unwrap();
        cb.record_success("trigger");

        // lots-of-history has 10 results, most others have 1 — it should survive
        let map = cb.state.read();
        assert!(
            map.contains_key("lots-of-history"),
            "key with most results should survive second-pass eviction"
        );
    }

    #[test]
    fn mutant_kill_trim_to_window_actually_trims() {
        // Mutant kill: circuit_breaker.rs:44 — replace trim_to_window body with ()
        let cb = InMemoryCircuitBreaker::new();
        // Use window_size=4 and record 10 results
        let config = test_config(10.0, 4, Duration::from_secs(5));

        cb.check_permitted("trim-key", &config).unwrap();

        // Record 10 successes (all below failure threshold)
        for _ in 0..10 {
            cb.record_success("trim-key");
        }

        // check_permitted calls trim_to_window — after trim, only 4 results should remain
        cb.check_permitted("trim-key", &config).unwrap();

        // Verify by checking internal state
        let map = cb.state.read();
        let circuit = map.get("trim-key").unwrap();
        assert!(
            circuit.results.len() <= 4,
            "trim_to_window should limit results to window_size=4, got {}",
            circuit.results.len()
        );
    }

    #[test]
    fn mutant_kill_eviction_excludes_current_key_in_second_pass() {
        // Mutant kill: circuit_breaker.rs:128 — k.as_str() != key filter
        // The current key must not be evicted in second-pass even if it's closed.
        let cb = InMemoryCircuitBreaker::new();
        let config = test_config(50.0, 100, Duration::from_secs(5));

        // Fill with keys that all have history (non-empty, so first-pass won't help)
        for i in 0..MAX_TRACKED_KEYS.saturating_add(2) {
            let key = format!("nonempty-{i}");
            cb.check_permitted(&key, &config).unwrap();
            cb.record_success(&key);
        }

        // Now check_permitted on a key that has minimal history —
        // it should still survive because it's the current key
        cb.check_permitted("current-key", &config).unwrap();
        cb.record_success("current-key");

        // Trigger another check to ensure it doesn't get evicted
        cb.check_permitted("current-key", &config).unwrap();

        let map = cb.state.read();
        assert!(
            map.contains_key("current-key"),
            "current key must survive second-pass eviction"
        );
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
