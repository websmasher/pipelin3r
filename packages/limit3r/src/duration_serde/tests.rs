#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: deserializing test fixtures"
)]

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Helper struct to test the `duration_serde` module via serde attributes.
#[derive(Debug, Serialize, Deserialize)]
struct Wrapper {
    #[serde(with = "super")]
    dur: Duration,
}

#[test]
fn round_trip_positive_duration() {
    let w = Wrapper {
        dur: Duration::from_secs_f64(1.5),
    };
    let json = serde_json::to_string(&w).unwrap();
    let deserialized: Wrapper = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.dur, Duration::from_secs_f64(1.5));
}

#[test]
fn round_trip_zero_duration() {
    let w = Wrapper {
        dur: Duration::ZERO,
    };
    let json = serde_json::to_string(&w).unwrap();
    let deserialized: Wrapper = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.dur, Duration::ZERO);
}

#[test]
fn rejects_negative_duration() {
    let json = r#"{"dur": -1.0}"#;
    let result = serde_json::from_str::<Wrapper>(json);
    assert!(result.is_err(), "negative duration should be rejected");
}

#[test]
fn rejects_nan_duration() {
    // JSON doesn't have NaN, but we test the deserializer path
    // by using a string that some parsers might accept. Since
    // serde_json rejects NaN natively, we test via a manual approach.
    let json = r#"{"dur": "NaN"}"#;
    let result = serde_json::from_str::<Wrapper>(json);
    assert!(result.is_err(), "NaN duration should be rejected");
}

#[test]
fn regression_duration_serde_rejects_negative_without_panic() {
    // Regression: deserializing a negative duration used to panic via
    // Duration::from_secs_f64(-1.0). After the fix, it returns an error.
    let json = r#"{"dur": -1.0}"#;
    let result = serde_json::from_str::<Wrapper>(json);
    assert!(
        result.is_err(),
        "negative duration must return error, not panic"
    );
}

#[test]
fn rejects_infinity_duration() {
    // serde_json rejects Infinity natively for f64
    let json = r#"{"dur": "Infinity"}"#;
    let result = serde_json::from_str::<Wrapper>(json);
    assert!(result.is_err(), "infinite duration should be rejected");
}
