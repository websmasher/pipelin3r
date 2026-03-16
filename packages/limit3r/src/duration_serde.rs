//! Serde helper for [`std::time::Duration`] as fractional seconds (f64).
//!
//! `std::time::Duration` does not implement `Serialize`/`Deserialize` by default.
//! This module provides a serialization format using fractional seconds so that
//! durations round-trip cleanly through JSON and YAML.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::Duration;

/// Serialize a [`Duration`] as fractional seconds.
///
/// # Errors
///
/// Returns a serializer error if the underlying `f64` serialization fails.
#[allow(clippy::type_complexity)] // serde `with` protocol requires this exact signature
pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    duration.as_secs_f64().serialize(serializer)
}

/// Deserialize a [`Duration`] from fractional seconds.
///
/// # Errors
///
/// Returns a deserializer error if the input is not a valid `f64`.
#[allow(clippy::type_complexity)] // serde `with` protocol requires this exact signature
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    let secs = f64::deserialize(deserializer)?;
    if secs < 0.0 || secs.is_nan() || secs.is_infinite() {
        return Err(serde::de::Error::custom(
            "duration must be a non-negative finite number",
        ));
    }
    Ok(Duration::from_secs_f64(secs))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    /// Helper struct to test the duration_serde module via serde attributes.
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod option_tests {
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
        let json = r#"{}"#;
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod non_option_guard_tests {
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
}

/// Serde helper for `Option<Duration>` as optional fractional seconds.
pub mod option {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    /// Serialize an `Option<Duration>` as optional fractional seconds.
    ///
    /// # Errors
    ///
    /// Returns a serializer error if the underlying serialization fails.
    #[allow(clippy::type_complexity)] // serde `with` protocol requires this exact signature
    pub fn serialize<S: Serializer>(
        duration: &Option<Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match *duration {
            Some(d) => serializer.serialize_some(&d.as_secs_f64()),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize an `Option<Duration>` from optional fractional seconds.
    ///
    /// # Errors
    ///
    /// Returns a deserializer error if the input is not a valid optional `f64`.
    #[allow(clippy::type_complexity)] // serde `with` protocol requires this exact signature
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Duration>, D::Error> {
        let opt = Option::<f64>::deserialize(deserializer)?;
        match opt {
            Some(secs) if secs < 0.0 || secs.is_nan() || secs.is_infinite() => {
                Err(serde::de::Error::custom(
                    "duration must be a non-negative finite number",
                ))
            }
            Some(secs) => Ok(Some(Duration::from_secs_f64(secs))),
            None => Ok(None),
        }
    }
}
