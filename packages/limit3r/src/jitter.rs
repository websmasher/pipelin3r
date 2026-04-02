//! Jitter utilities for randomizing timed delays.

use std::time::Duration;

use rand::Rng;

/// Conservative ceiling for Duration clamp (~31.7 billion years).
///
/// `Duration::MAX.as_secs_f64()` can round up past `u64::MAX` due to f64
/// precision loss, causing `Duration::from_secs_f64` to panic. This constant
/// is safely below the true maximum while being far larger than any practical
/// timeout.
const SAFE_MAX_SECS: f64 = 1e18;

/// Apply jitter to a duration.
///
/// Given a base duration and a jitter factor (0.0 to 1.0), returns a
/// random duration uniformly distributed in
/// `[base * (1 - factor), base * (1 + factor)]`.
///
/// A `jitter_factor` of 0.0 returns the base duration unchanged.
/// A `jitter_factor` of 0.5 returns a duration in `[base * 0.5, base * 1.5]`.
/// A `jitter_factor` of 1.0 returns a duration in `[0, base * 2.0]`.
///
/// # Panics
///
/// This function does not panic. Negative results from floating-point
/// imprecision are clamped to zero.
pub fn apply_jitter(base: Duration, jitter_factor: f64) -> Duration {
    // Defensive: non-finite or non-positive factors return base unchanged.
    // This covers NaN (all comparisons false), negative values, and infinity.
    // We check is_finite first, then the range — this avoids negated
    // comparison operators on partial-ord types.
    if !jitter_factor.is_finite() || jitter_factor <= 0.0 {
        return base;
    }

    let base_secs = base.as_secs_f64();

    #[allow(clippy::arithmetic_side_effects)] // f64 mul of bounded [0,1] factor
    let min_secs = base_secs * (1.0 - jitter_factor);
    #[allow(clippy::arithmetic_side_effects)] // f64 mul of bounded [0,2] factor
    let max_secs = base_secs * (1.0 + jitter_factor);

    // Clamp min to 0 (jitter_factor = 1.0 gives min = 0.0, but floating
    // point imprecision could yield a tiny negative).
    let min_clamped = if min_secs < 0.0 { 0.0 } else { min_secs };

    // Guard against degenerate range (e.g. base = 0) or non-finite results
    // from very large base durations.
    if min_clamped >= max_secs || !max_secs.is_finite() {
        return base;
    }

    let jittered = rand::rng().random_range(min_clamped..=max_secs);

    // Clamp to a value safely representable as Duration.
    let safe = if jittered > SAFE_MAX_SECS {
        SAFE_MAX_SECS
    } else {
        jittered
    };
    Duration::from_secs_f64(safe)
}

#[cfg(test)]
#[path = "jitter_tests.rs"]
mod tests;
