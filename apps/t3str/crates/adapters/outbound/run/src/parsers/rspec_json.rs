//! Parser for `RSpec` `--format json` output.
//!
//! `RSpec` produces a JSON object with an `examples` array, where each entry
//! describes a single test example with its status, duration, and optional
//! exception details.

use t3str_domain_types::{T3strError, TestResult, TestStatus};

use super::seconds_to_ms;

/// Parse `RSpec` JSON output into a list of test results.
///
/// # Errors
///
/// Returns `T3strError::ParseFailed` if the input is not valid JSON
/// or does not contain the expected `examples` array.
pub fn parse(json: &str) -> super::ParseResult {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| T3strError::ParseFailed {
            format: "rspec-json".to_owned(),
            reason: e.to_string(),
        })?;

    let examples = root
        .get("examples")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| T3strError::ParseFailed {
            format: "rspec-json".to_owned(),
            reason: "missing examples array".to_owned(),
        })?;

    let mut results = Vec::new();

    for example in examples {
        let name: &str = match example
            .get("full_description")
            .and_then(serde_json::Value::as_str)
        {
            Some(d) => d,
            None => continue,
        };

        let status_str = example
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        let status = match status_str {
            "passed" => TestStatus::Passed,
            "failed" => TestStatus::Failed,
            "pending" => TestStatus::Skipped,
            _ => continue,
        };

        let duration_ms = example
            .get("run_time")
            .and_then(serde_json::Value::as_f64)
            .and_then(seconds_to_ms);

        let message = example
            .get("exception")
            .and_then(|exc: &serde_json::Value| exc.get("message"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);

        let file = example
            .get("file_path")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);

        results.push(TestResult {
            name: name.to_owned(),
            status,
            duration_ms,
            message,
            file,
        });
    }

    Ok(results)
}

#[cfg(test)]
#[path = "rspec_json_tests.rs"]
mod tests;
