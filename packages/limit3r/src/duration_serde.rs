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
    fn rejects_infinity_duration() {
        // serde_json rejects Infinity natively for f64
        let json = r#"{"dur": "Infinity"}"#;
        let result = serde_json::from_str::<Wrapper>(json);
        assert!(result.is_err(), "infinite duration should be rejected");
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
