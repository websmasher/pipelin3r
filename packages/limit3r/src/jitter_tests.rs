#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]

use super::*;

#[test]
fn zero_factor_returns_exact_base() {
    let base = Duration::from_millis(500);
    let result = apply_jitter(base, 0.0);
    assert_eq!(result, base);
}

#[test]
fn result_within_expected_bounds() {
    let base = Duration::from_secs(2);
    let factor = 0.5;
    // Expected range: [1.0s, 3.0s]
    for _ in 0..200 {
        let result = apply_jitter(base, factor);
        assert!(
            result >= Duration::from_secs(1),
            "jittered duration {result:?} below lower bound 1s"
        );
        assert!(
            result <= Duration::from_secs(3),
            "jittered duration {result:?} above upper bound 3s"
        );
    }
}

#[test]
fn factor_one_can_reach_near_zero() {
    let base = Duration::from_secs(1);
    // With factor=1.0, range is [0, 2s]. Over many runs we should see
    // values below 0.5s at least once.
    let mut saw_low = false;
    for _ in 0..500 {
        let result = apply_jitter(base, 1.0);
        assert!(
            result <= Duration::from_secs(2),
            "jittered duration {result:?} above upper bound 2s"
        );
        if result < Duration::from_millis(500) {
            saw_low = true;
        }
    }
    assert!(
        saw_low,
        "factor=1.0 never produced a value below 0.5s in 500 iterations"
    );
}

#[test]
fn produces_variable_output() {
    let base = Duration::from_secs(1);
    let factor = 0.5;
    let mut results = std::collections::BTreeSet::new();
    for _ in 0..100 {
        let millis = apply_jitter(base, factor).as_millis();
        let _new = results.insert(millis);
    }
    // With uniform random in [500ms, 1500ms], we should see many distinct
    // millisecond values over 100 trials.
    assert!(
        results.len() > 10,
        "expected variable output, got only {} distinct values",
        results.len()
    );
}

#[test]
fn zero_base_returns_zero() {
    let result = apply_jitter(Duration::ZERO, 0.5);
    assert_eq!(result, Duration::ZERO);
}

#[test]
fn negative_factor_returns_base() {
    // Validation should prevent this, but the function handles it gracefully.
    let base = Duration::from_secs(1);
    let result = apply_jitter(base, -0.5);
    assert_eq!(result, base);
}

// --- Adversarial edge cases ---

#[test]
fn nan_factor_returns_base_without_panic() {
    let base = Duration::from_secs(1);
    let result = apply_jitter(base, f64::NAN);
    assert_eq!(result, base, "NaN factor must return base, not panic");
}

#[test]
fn infinity_factor_returns_base_without_panic() {
    let base = Duration::from_secs(1);
    let result = apply_jitter(base, f64::INFINITY);
    assert_eq!(result, base, "INFINITY factor must return base, not panic");
}

#[test]
fn neg_infinity_factor_returns_base_without_panic() {
    let base = Duration::from_secs(1);
    let result = apply_jitter(base, f64::NEG_INFINITY);
    assert_eq!(
        result, base,
        "NEG_INFINITY factor must return base, not panic"
    );
}

#[test]
fn very_large_base_does_not_panic() {
    // Duration::MAX with any factor > 0 could overflow in from_secs_f64.
    // The function must clamp, not panic.
    let result = apply_jitter(Duration::MAX, 0.5);
    // Result should be approximately Duration::MAX (clamped).
    assert!(
        result >= Duration::from_secs(1),
        "very large base should produce a large result, got {result:?}"
    );
}

#[test]
fn nanosecond_base_with_full_jitter() {
    let base = Duration::from_nanos(1);
    for _ in 0..100 {
        let result = apply_jitter(base, 1.0);
        assert!(
            result <= Duration::from_nanos(2),
            "1ns base with factor=1.0 should produce at most 2ns, got {result:?}"
        );
    }
}
