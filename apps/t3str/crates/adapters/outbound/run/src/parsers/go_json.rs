//! Parser for `go test -json` NDJSON output.
//!
//! Each line of Go's JSON test output is an independent JSON object describing
//! a test event. This parser extracts terminal events (pass, fail, skip) and
//! converts them to [`TestResult`].

use std::collections::BTreeSet;

use t3str_domain_types::{TestResult, TestStatus};

use super::seconds_to_ms;

/// Dedup key: (package, test name).
type SeenKey = (String, String);

/// Parse `go test -json` NDJSON output into a list of test results.
///
/// # Errors
///
/// Returns `T3strError::ParseFailed` if no valid test events could be
/// extracted, though individual malformed lines are silently skipped.
pub fn parse(input: &str) -> super::ParseResult {
    let mut results = Vec::new();
    let mut seen: BTreeSet<SeenKey> = BTreeSet::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // skip non-JSON lines
        };

        // Only process events that have a Test field
        let test_name = match value.get("Test").and_then(serde_json::Value::as_str) {
            Some(name) if !name.is_empty() => name,
            _ => continue,
        };

        // Only process terminal actions
        let Some(action) = value.get("Action").and_then(serde_json::Value::as_str) else {
            continue;
        };

        let status = match action {
            "pass" => TestStatus::Passed,
            "fail" => TestStatus::Failed,
            "skip" => TestStatus::Skipped,
            _ => continue, // ignore "run", "output", "pause", "cont"
        };

        let package = value
            .get("Package")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        // Dedup by (Package, Test) — keep first terminal event
        let key = (package.to_owned(), test_name.to_owned());
        if !seen.insert(key) {
            continue;
        }

        let duration_ms = value
            .get("Elapsed")
            .and_then(serde_json::Value::as_f64)
            .and_then(seconds_to_ms);

        let mut name = String::with_capacity(
            package
                .len()
                .saturating_add(test_name.len())
                .saturating_add(2),
        );
        name.push_str(package);
        name.push_str("::");
        name.push_str(test_name);

        results.push(TestResult {
            name,
            status,
            duration_ms,
            message: None,
            file: None,
        });
    }

    Ok(results)
}

#[cfg(test)]
#[path = "go_json_tests.rs"]
mod tests;
