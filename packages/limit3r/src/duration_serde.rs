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
