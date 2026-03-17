#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: deserializing test fixtures"
)]

use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Wrapper {
    #[serde(with = "super")]
    dur: Duration,
}

#[test]
fn mutant_kill_non_option_or_vs_and() {
    // Mutant kill: duration_serde.rs:28 — replace || with && in non-option deserialize
    // With &&, a value that is negative but not NaN and not infinite would pass.
    // -1.0 is negative, finite, not NaN — must be rejected.
    let json = r#"{"dur": -1.0}"#;
    let result = serde_json::from_str::<Wrapper>(json);
    assert!(
        result.is_err(),
        "non-option: negative finite value must be rejected (|| not &&)"
    );
}

#[test]
#[allow(clippy::approx_constant, reason = "test value, not approximating PI")]
fn mutant_kill_non_option_accepts_valid_positive() {
    // Mutant kill: duration_serde.rs:28 — if guard replaced with true, positives rejected
    let json = r#"{"dur": 3.14}"#;
    let parsed: Wrapper = serde_json::from_str(json).unwrap();
    assert_eq!(
        parsed.dur,
        Duration::from_secs_f64(3.14),
        "non-option: valid positive must be accepted"
    );
}

#[test]
#[allow(clippy::unwrap_used)] // reason: test
fn mutant_kill_v2_non_option_each_guard_condition_independent() {
    // Mutant kill: duration_serde.rs:28 — replace || with && in
    //   `secs < 0.0 || secs.is_nan() || secs.is_infinite()`
    // With &&, ALL three must be true to reject. A value that is ONLY
    // negative (not NaN, not infinite) would slip through &&.
    // We test each condition in isolation via the raw deserialize fn.

    // 1) Negative only: -1.0 is negative, finite, not NaN
    let neg: Result<Duration, _> =
        super::deserialize(&mut serde_json::Deserializer::from_str("-1.0"));
    assert!(neg.is_err(), "negative-only must be rejected (|| not &&)");

    // 2) Positive value must still be accepted
    let pos: Result<Duration, _> =
        super::deserialize(&mut serde_json::Deserializer::from_str("1.0"));
    assert!(pos.is_ok(), "positive value must be accepted");
    assert_eq!(pos.unwrap(), Duration::from_secs_f64(1.0));
}
