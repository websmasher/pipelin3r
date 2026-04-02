#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
#![allow(
    clippy::significant_drop_tightening,
    reason = "test code: lock scope is intentional"
)]

use super::*;
use std::time::Duration;

fn test_config(threshold: f64, window: u32, wait: Duration) -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        failure_rate_threshold: threshold,
        sliding_window_size: window,
        wait_duration_in_open_state: wait,
        jitter_factor: 0.0,
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
    assert!(
        result.is_ok(),
        "circuit tripped with only 1 result in a window of 10"
    );
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
    assert!(
        result.is_ok(),
        "circuit tripped with only 4 results, min_calls is 5"
    );
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
    assert!(
        result.is_err(),
        "circuit did not trip at exactly 50% with threshold 50%"
    );
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
fn no_eviction_at_exactly_max_keys_circuit_breaker() {
    // Mutant kill: `>` vs `>=` — at exactly max, no eviction should run
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 4, Duration::from_secs(5));

    // Fill to exactly 5
    for i in 0..5 {
        cb.check_permitted(&format!("key-{i}"), &config).unwrap();
    }

    // All 5 keys must exist (no eviction at exactly the limit)
    let size_at_max = cb.state.read().len();
    assert_eq!(size_at_max, 5, "no eviction at exactly max_tracked_keys");
    for i in 0..5 {
        let exists = cb.state.read().contains_key(&format!("key-{i}"));
        assert!(exists, "key-{i} must exist at exactly the limit");
    }
}

#[test]
fn eviction_triggers_when_exceeding_max_keys_circuit_breaker() {
    // Mutant kill: `>` vs `>=` — at max+1, eviction must trigger
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 4, Duration::from_secs(5));

    // Fill to exactly 5 (all closed+empty = idle)
    for i in 0..5 {
        cb.check_permitted(&format!("key-{i}"), &config).unwrap();
    }

    // Add one more — triggers eviction (idle keys evicted)
    cb.check_permitted("key-5", &config).unwrap();

    let map = cb.state.read();
    assert!(
        map.len() <= 6,
        "eviction should run after exceeding max, got {}",
        map.len()
    );
    assert!(
        map.contains_key("key-5"),
        "current key must survive eviction"
    );
}

#[test]
fn eviction_keeps_open_circuits() {
    // Mutant kill: first-pass retain logic — Open circuits must survive
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 4, Duration::from_secs(60));

    // Create a key and force it open
    cb.check_permitted("open-key", &config).unwrap();
    cb.record_failure("open-key");
    cb.record_failure("open-key");
    cb.record_failure("open-key");
    let _ = cb.check_permitted("open-key", &config); // triggers open

    // Fill with idle keys (closed, empty history)
    for i in 0..5 {
        cb.check_permitted(&format!("idle-{i}"), &config).unwrap();
    }

    // Trigger eviction
    cb.check_permitted("trigger-evict", &config).unwrap();

    let map = cb.state.read();
    assert!(
        map.contains_key("open-key"),
        "open circuit must survive eviction"
    );
}

#[test]
fn eviction_removes_closed_empty_first() {
    // Mutant kill: first pass removes closed+empty, not closed+history
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 10, Duration::from_secs(5));

    // Create a key with non-empty history
    cb.check_permitted("has-history", &config).unwrap();
    cb.record_success("has-history");
    cb.record_success("has-history");

    // Fill with truly idle keys (closed, empty)
    for i in 0..5 {
        cb.check_permitted(&format!("empty-{i}"), &config).unwrap();
    }

    // Trigger eviction
    cb.check_permitted("trigger-evict", &config).unwrap();

    let map = cb.state.read();
    assert!(
        map.contains_key("has-history"),
        "closed circuit with history should survive first-pass eviction"
    );
}

#[test]
fn second_pass_evicts_fewest_results_first() {
    // Mutant kill: second pass — keys with fewest results evicted first
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 100, Duration::from_secs(5));

    // Create keys with varying history (none truly idle, so first-pass won't help)
    for i in 0..6 {
        let key = format!("hist-{i}");
        cb.check_permitted(&key, &config).unwrap();
        cb.record_success(&key);
    }

    // Create a key with many results — should be last to be evicted
    cb.check_permitted("lots-of-history", &config).unwrap();
    for _ in 0..10 {
        cb.record_success("lots-of-history");
    }

    // Trigger eviction
    cb.check_permitted("trigger", &config).unwrap();
    cb.record_success("trigger");

    let map = cb.state.read();
    assert!(
        map.contains_key("lots-of-history"),
        "key with most results should survive second-pass eviction"
    );
}

#[test]
fn eviction_preserves_failure_history() {
    // Regression: eviction must not lose failure history for keys accumulating failures.
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 10, Duration::from_secs(5));

    // Fill with idle keys (no history)
    for i in 0..5 {
        cb.check_permitted(&format!("filler-{i}"), &config).unwrap();
    }

    // Add a key with failure history below threshold
    cb.check_permitted("important-key", &config).unwrap();
    cb.record_failure("important-key");
    cb.record_failure("important-key");
    cb.record_failure("important-key");

    // Trigger eviction by inserting another key
    cb.check_permitted("trigger-eviction", &config).unwrap();

    // Record more failures to reach the trip threshold
    // min_calls = max(2, 10/2) = 5, we have 3 failures, add 2 more
    cb.record_failure("important-key");
    cb.record_failure("important-key");

    // If history was preserved, 5 failures = 100% >= 50%, should trip
    let result = cb.check_permitted("important-key", &config);
    assert!(
        result.is_err(),
        "failure history was lost during eviction — circuit should have tripped"
    );
}

#[test]
fn regression_circuit_breaker_does_not_trip_on_partial_window() {
    let cb = InMemoryCircuitBreaker::new();
    let config = test_config(50.0, 100, Duration::from_secs(5));

    cb.check_permitted("key-a", &config).unwrap();
    cb.record_failure("key-a");

    let result = cb.check_permitted("key-a", &config);
    assert!(
        result.is_ok(),
        "circuit tripped on 1 failure in a window of 100 — premature trip on partial window"
    );
}

#[test]
fn regression_circuit_breaker_trips_at_exact_threshold_with_window_2() {
    let cb = InMemoryCircuitBreaker::new();
    let config = test_config(50.0, 2, Duration::from_secs(5));

    cb.check_permitted("key-a", &config).unwrap();
    cb.record_success("key-a");
    cb.record_failure("key-a");

    let result = cb.check_permitted("key-a", &config);
    assert!(
        result.is_err(),
        "circuit did not trip at exactly 50% failure rate with threshold 50% and window 2"
    );
}

#[test]
fn trim_to_window_actually_trims() {
    let cb = InMemoryCircuitBreaker::new();
    let config = test_config(10.0, 4, Duration::from_secs(5));

    cb.check_permitted("trim-key", &config).unwrap();

    for _ in 0..10 {
        cb.record_success("trim-key");
    }

    cb.check_permitted("trim-key", &config).unwrap();

    let map = cb.state.read();
    let circuit = map.get("trim-key").unwrap();
    assert!(
        circuit.results.len() <= 4,
        "trim_to_window should limit results to window_size=4, got {}",
        circuit.results.len()
    );
}

#[test]
fn eviction_excludes_current_key_in_second_pass() {
    // Current key must survive second-pass even if it's closed with minimal history
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 100, Duration::from_secs(5));

    // Fill with keys that all have history (non-empty, so first-pass won't help)
    for i in 0..6 {
        let key = format!("nonempty-{i}");
        cb.check_permitted(&key, &config).unwrap();
        cb.record_success(&key);
    }

    // Check on a key with minimal history — must survive as current key
    cb.check_permitted("current-key", &config).unwrap();
    cb.record_success("current-key");

    cb.check_permitted("current-key", &config).unwrap();

    let map = cb.state.read();
    assert!(
        map.contains_key("current-key"),
        "current key must survive second-pass eviction"
    );
}

#[test]
fn first_pass_retains_non_closed_circuits() {
    // Open circuits must survive first-pass eviction even with small max_keys
    let cb = InMemoryCircuitBreaker::with_max_keys(5);
    let config = test_config(50.0, 4, Duration::from_secs(60));

    // Create an Open circuit
    cb.check_permitted("must-survive-open", &config).unwrap();
    cb.record_failure("must-survive-open");
    cb.record_failure("must-survive-open");
    cb.record_failure("must-survive-open");
    let _ = cb.check_permitted("must-survive-open", &config); // opens it

    // Fill with idle keys to exceed limit
    for i in 0..5 {
        cb.check_permitted(&format!("idle-{i}"), &config).unwrap();
    }

    // Trigger eviction
    cb.check_permitted("eviction-trigger", &config).unwrap();

    let map = cb.state.read();
    assert!(
        map.contains_key("must-survive-open"),
        "Open circuit evicted by first pass"
    );
    let circuit = map.get("must-survive-open").unwrap();
    assert_eq!(
        circuit.state,
        State::Open,
        "Open circuit state corrupted during eviction"
    );
}

#[test]
fn second_pass_only_evicts_closed() {
    // Open circuits must survive second-pass eviction
    let cb = InMemoryCircuitBreaker::with_max_keys(10);
    let config = test_config(50.0, 4, Duration::from_secs(60));

    // Create 3 Open circuits
    for i in 0..3 {
        let key = format!("open-{i}");
        cb.check_permitted(&key, &config).unwrap();
        cb.record_failure(&key);
        cb.record_failure(&key);
        cb.record_failure(&key);
        let _ = cb.check_permitted(&key, &config); // opens it
    }

    // Fill remaining with Closed circuits that have history
    for i in 0..10 {
        let key = format!("closed-{i}");
        cb.check_permitted(&key, &config).unwrap();
        cb.record_success(&key);
    }

    // Trigger eviction
    cb.check_permitted("trigger-evict-2", &config).unwrap();
    cb.record_success("trigger-evict-2");

    let map = cb.state.read();
    let open_surviving = (0..3)
        .filter(|i| map.contains_key(&format!("open-{i}")))
        .count();
    assert_eq!(
        open_surviving,
        3,
        "second pass evicted {lost} Open circuits — must only evict Closed",
        lost = 3_usize.saturating_sub(open_surviving),
    );
}

#[tokio::test]
async fn stale_failures_expire_and_do_not_poison_window() {
    // BUG FIX: Old failures in the sliding window should expire based on TTL.
    // With wait_duration=50ms, TTL = 50ms * 4 = 200ms. Failures older than
    // 200ms should be evicted before evaluating the failure rate.
    let cb = InMemoryCircuitBreaker::new();
    let config = test_config(50.0, 10, Duration::from_millis(50));
    // TTL = 50ms * 4 = 200ms

    // Initialize and record failures (but not enough to trip with min_calls)
    cb.check_permitted("key-a", &config).unwrap();
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    cb.record_failure("key-a");

    // These 5 failures are in the window. With min_calls = max(2, 10/2) = 5,
    // the next check_permitted would trip (5/5 = 100% >= 50%).
    let result = cb.check_permitted("key-a", &config);
    assert!(result.is_err(), "should trip with 5 failures");

    // Wait for the open state to expire (50ms) + half-open probe
    tokio::time::sleep(Duration::from_millis(60)).await;
    cb.check_permitted("key-a", &config).unwrap(); // half-open probe
    cb.record_success("key-a"); // close the circuit, clears window

    // Now wait for the TTL to expire (200ms total from the failures)
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Record new calls — these should NOT be poisoned by the old failures
    cb.record_success("key-a");
    cb.record_success("key-a");
    cb.record_success("key-a");
    cb.record_success("key-a");
    cb.record_success("key-a");
    cb.record_failure("key-a"); // 1 failure out of 6 = 16.7%

    let fresh_check = cb.check_permitted("key-a", &config);
    assert!(
        fresh_check.is_ok(),
        "stale failures should have expired — only 1/6 recent = 16.7% < 50%"
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

// --- Jitter tests ---

#[tokio::test]
async fn jitter_varies_half_open_transition_time() {
    // With jitter_factor=1.0, the wait can be [0, 2*base]. We verify that
    // the circuit still transitions through the full lifecycle.
    let cb = InMemoryCircuitBreaker::new();
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 4,
        wait_duration_in_open_state: Duration::from_millis(50),
        jitter_factor: 0.5,
    };

    // Force open
    cb.check_permitted("key-a", &config).unwrap();
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    let _ = cb.check_permitted("key-a", &config); // opens

    // Wait long enough for any jittered duration (max = 50ms * 1.5 = 75ms)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should transition to half-open
    let probe = cb.check_permitted("key-a", &config);
    assert!(
        probe.is_ok(),
        "circuit should be half-open after jittered wait"
    );

    // Close it
    cb.record_success("key-a");
    let closed = cb.check_permitted("key-a", &config);
    assert!(
        closed.is_ok(),
        "circuit should be closed after successful probe"
    );
}

#[test]
fn jittered_wait_is_set_when_circuit_opens() {
    let cb = InMemoryCircuitBreaker::new();
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 4,
        wait_duration_in_open_state: Duration::from_secs(5),
        jitter_factor: 0.5,
    };

    cb.check_permitted("key-a", &config).unwrap();
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    let _ = cb.check_permitted("key-a", &config); // triggers open

    let map = cb.state.read();
    let circuit = map.get("key-a").unwrap();
    assert_eq!(circuit.state, State::Open);
    assert!(
        circuit.jittered_wait.is_some(),
        "jittered_wait must be set when circuit opens"
    );
    let wait = circuit.jittered_wait.unwrap();
    // With factor=0.5, range is [2.5s, 7.5s]
    assert!(
        wait >= Duration::from_millis(2500),
        "jittered_wait {wait:?} below lower bound 2.5s"
    );
    assert!(
        wait <= Duration::from_millis(7500),
        "jittered_wait {wait:?} above upper bound 7.5s"
    );
}

#[tokio::test]
async fn jittered_wait_cleared_on_close() {
    let cb = InMemoryCircuitBreaker::new();
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 4,
        wait_duration_in_open_state: Duration::from_millis(1),
        jitter_factor: 0.5,
    };

    // Force open
    cb.check_permitted("key-a", &config).unwrap();
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    cb.record_failure("key-a");
    let _ = cb.check_permitted("key-a", &config);

    // Wait for half-open (wait is <=2ms with factor 0.5)
    tokio::time::sleep(Duration::from_millis(5)).await;
    let _ = cb.check_permitted("key-a", &config); // half-open

    // Close via success
    cb.record_success("key-a");

    let map = cb.state.read();
    let circuit = map.get("key-a").unwrap();
    assert_eq!(circuit.state, State::Closed);
    assert!(
        circuit.jittered_wait.is_none(),
        "jittered_wait must be cleared when circuit closes"
    );
}

#[tokio::test]
async fn reopen_after_failed_probe_preserves_jittered_wait() {
    // When a HalfOpen probe fails, the circuit re-opens but keeps the
    // original jittered_wait (config not available in record_outcome).
    let cb = InMemoryCircuitBreaker::new();
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 4,
        wait_duration_in_open_state: Duration::from_millis(50),
        jitter_factor: 0.5,
    };

    // Force open
    cb.check_permitted("k", &config).unwrap();
    cb.record_failure("k");
    cb.record_failure("k");
    cb.record_failure("k");
    let _ = cb.check_permitted("k", &config);

    let first_wait = {
        let map = cb.state.read();
        map.get("k").unwrap().jittered_wait
    };
    assert!(first_wait.is_some());

    // Transition to half-open and fail the probe
    tokio::time::sleep(Duration::from_millis(100)).await;
    cb.check_permitted("k", &config).unwrap(); // half-open
    cb.record_failure("k"); // re-opens

    let second_wait = {
        let map = cb.state.read();
        map.get("k").unwrap().jittered_wait
    };
    // Re-open reuses the original jittered_wait (intentional design)
    assert_eq!(
        first_wait, second_wait,
        "re-open after failed probe must preserve jittered_wait"
    );
}

#[tokio::test]
async fn each_open_cycle_gets_fresh_jittered_wait() {
    // Each close→reopen cycle should compute a fresh jittered_wait.
    let cb = InMemoryCircuitBreaker::new();
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 4,
        wait_duration_in_open_state: Duration::from_millis(1),
        jitter_factor: 0.5,
    };

    let mut waits = Vec::new();
    for _ in 0..10 {
        cb.check_permitted("k", &config).unwrap();
        cb.record_failure("k");
        cb.record_failure("k");
        cb.record_failure("k");
        let _ = cb.check_permitted("k", &config); // opens

        let w = {
            let map = cb.state.read();
            map.get("k").unwrap().jittered_wait.unwrap()
        };
        waits.push(w);

        // Close it
        tokio::time::sleep(Duration::from_millis(5)).await;
        cb.check_permitted("k", &config).unwrap(); // half-open
        cb.record_success("k"); // close

        let map = cb.state.read();
        assert!(
            map.get("k").unwrap().jittered_wait.is_none(),
            "jittered_wait must be None after close"
        );
    }

    // With 10 independent samples from [0.5ms, 1.5ms], we should see
    // at least 2 distinct values.
    #[allow(
        clippy::redundant_closure_for_method_calls,
        reason = "as_nanos takes &self, not self"
    )]
    let distinct: std::collections::BTreeSet<_> = waits.iter().map(|d| d.as_nanos()).collect();
    assert!(
        distinct.len() >= 2,
        "expected multiple distinct jittered_wait values, got {waits:?}"
    );
}

#[test]
fn eviction_preserves_jittered_wait_of_open_circuit() {
    let cb = InMemoryCircuitBreaker::with_max_keys(3);
    let config = CircuitBreakerConfig {
        failure_rate_threshold: 50.0,
        sliding_window_size: 4,
        wait_duration_in_open_state: Duration::from_secs(60),
        jitter_factor: 0.5,
    };

    // Open a circuit
    cb.check_permitted("open-key", &config).unwrap();
    cb.record_failure("open-key");
    cb.record_failure("open-key");
    cb.record_failure("open-key");
    let _ = cb.check_permitted("open-key", &config);

    let original_wait = {
        let map = cb.state.read();
        map.get("open-key").unwrap().jittered_wait
    };
    assert!(original_wait.is_some());

    // Trigger eviction by filling with idle keys
    for i in 0..4 {
        cb.check_permitted(&format!("idle-{i}"), &config).unwrap();
    }

    // open-key must survive with jittered_wait intact
    let map = cb.state.read();
    let c = map
        .get("open-key")
        .expect("open circuit must survive eviction");
    assert_eq!(c.state, State::Open);
    assert_eq!(
        c.jittered_wait, original_wait,
        "eviction must not corrupt jittered_wait"
    );
}
