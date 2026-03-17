#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: deserializing test fixtures"
)]

use serde::Deserialize;
use std::time::Duration;

/// Helper struct for testing `option::deserialize`.
#[derive(Debug, Deserialize)]
struct OptDuration {
    #[serde(default, with = "super::option")]
    d: Option<Duration>,
}

#[test]
fn mutant_kill_option_deserialize_some_value() {
    // Mutant kill: duration_serde.rs:144 — replace option::deserialize return with Ok(None) or Ok(Some(Default))
    let json = r#"{"d": 1.5}"#;
    let parsed: OptDuration = serde_json::from_str(json).unwrap();
    assert_eq!(
        parsed.d,
        Some(Duration::from_secs_f64(1.5)),
        "option deserialize must return the correct Duration value"
    );
}

#[test]
fn mutant_kill_option_deserialize_null() {
    // Mutant kill: duration_serde.rs:145 — replace option::deserialize return with Ok(Some(Default))
    let json = r#"{"d": null}"#;
    let parsed: OptDuration = serde_json::from_str(json).unwrap();
    assert_eq!(
        parsed.d, None,
        "option deserialize must return None for null"
    );
}

#[test]
fn mutant_kill_option_deserialize_missing_field() {
    // Mutant kill: duration_serde.rs:145 — replace Ok(None) with Ok(Some(Default))
    let json = r"{}";
    let parsed: OptDuration = serde_json::from_str(json).unwrap();
    assert_eq!(
        parsed.d, None,
        "option deserialize must return None for missing field"
    );
}

#[test]
fn mutant_kill_option_rejects_negative() {
    // Mutant kill: duration_serde.rs:139 — replace < with == or >
    let json = r#"{"d": -1.0}"#;
    let result = serde_json::from_str::<OptDuration>(json);
    assert!(
        result.is_err(),
        "option deserialize must reject negative values"
    );
}

#[test]
fn mutant_kill_option_rejects_negative_large() {
    // Mutant kill: duration_serde.rs:139 — replace < with <=
    // -100.0 is clearly negative, kills < vs <= vs == vs > mutations
    let json = r#"{"d": -100.0}"#;
    let result = serde_json::from_str::<OptDuration>(json);
    assert!(
        result.is_err(),
        "option deserialize must reject large negative values"
    );
}

#[test]
fn mutant_kill_option_accepts_zero() {
    // Mutant kill: duration_serde.rs:139 — replace < with <= would reject zero
    let json = r#"{"d": 0.0}"#;
    let parsed: OptDuration = serde_json::from_str(json).unwrap();
    assert_eq!(
        parsed.d,
        Some(Duration::ZERO),
        "option deserialize must accept zero"
    );
}

#[test]
fn mutant_kill_option_guard_all_conditions_independent() {
    // Mutant kill: duration_serde.rs:139 — replace || with &&
    // With && instead of ||, a negative non-NaN non-infinite value would pass.
    // This test ensures negative alone is rejected (tests || vs &&).
    let json = r#"{"d": -0.5}"#;
    let result = serde_json::from_str::<OptDuration>(json);
    assert!(
        result.is_err(),
        "negative-only value must be rejected (|| not &&)"
    );
}

#[test]
fn mutant_kill_option_guard_replaced_with_true() {
    // Mutant kill: duration_serde.rs:139 — replace whole guard with true
    // If guard is always true, even valid positive values would be rejected.
    let json = r#"{"d": 2.5}"#;
    let parsed: OptDuration = serde_json::from_str(json).unwrap();
    assert_eq!(
        parsed.d,
        Some(Duration::from_secs_f64(2.5)),
        "valid positive value must be accepted (guard not always true)"
    );
}

#[test]
fn mutant_kill_option_guard_replaced_with_false() {
    // Mutant kill: duration_serde.rs:139 — replace whole guard with false
    // If guard is always false, negative values would be accepted.
    let json = r#"{"d": -5.0}"#;
    let result = serde_json::from_str::<OptDuration>(json);
    assert!(
        result.is_err(),
        "negative value must be rejected (guard not always false)"
    );
}

#[test]
fn mutant_kill_v2_option_each_guard_condition_independent() {
    // Mutant kill: duration_serde.rs:298 — replace || with && in
    //   `secs < 0.0 || secs.is_nan() || secs.is_infinite()`
    // With &&, a value that is ONLY negative passes because the other
    // two conditions are false. With ||, any single true rejects.
    // Call the raw option::deserialize to bypass serde_json limitations.

    // -2.0 is negative, finite, not NaN → only first condition true
    let neg: Result<Option<Duration>, _> =
        super::option::deserialize(&mut serde_json::Deserializer::from_str("-2.0"));
    assert!(
        neg.is_err(),
        "option: negative-only must be rejected (|| not &&)"
    );

    // Positive must still work
    let pos: Result<Option<Duration>, _> =
        super::option::deserialize(&mut serde_json::Deserializer::from_str("2.0"));
    assert_eq!(
        pos.expect("positive must succeed"),
        Some(Duration::from_secs_f64(2.0)),
    );
}
