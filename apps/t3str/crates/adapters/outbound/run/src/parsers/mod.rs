//! Test output parsers for various formats.
//!
//! These parsers convert structured test runner output (`JUnit` XML, Go JSON,
//! etc.) into the unified `TestResult` format.

pub mod cargo_text;
pub mod dotnet_stdout;
pub mod go_json;
pub mod jest_json;
pub mod junit_xml;
pub mod mix_text;
pub mod mocha_text;
pub mod nette_text;
pub mod rspec_json;

use t3str_domain_types::{T3strError, TestResult, TestStatus};

/// Result type returned by all parsers.
pub type ParseResult = Result<Vec<TestResult>, T3strError>;

/// A child element's contribution: optional status override and optional message.
pub(crate) type ChildOutcome = Result<(Option<TestStatus>, Option<String>), T3strError>;

/// Result type for extracting an optional string value.
pub(crate) type OptStringResult = Result<Option<String>, T3strError>;

/// Convert a duration in seconds (f64) to milliseconds (u64).
///
/// Returns `None` for negative, infinite, or NaN values.
#[allow(clippy::as_conversions)] // duration is always a small positive float from XML/JSON
#[allow(clippy::cast_possible_truncation)] // ms value fits in u64 (max ~584M years)
#[allow(clippy::cast_sign_loss)] // guarded by >= 0.0 check above
pub fn seconds_to_ms(seconds: f64) -> Option<u64> {
    if seconds.is_finite() && seconds >= 0.0 {
        Some(seconds.mul_add(1000.0, 0.5).floor() as u64)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
