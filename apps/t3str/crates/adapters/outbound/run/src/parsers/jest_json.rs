//! Parser for Jest `--json` output.
//!
//! Jest produces a JSON object with a `testResults` array, where each entry
//! represents a test file and contains either `testResults` or
//! `assertionResults` with individual test outcomes.

use t3str_domain_types::{T3strError, TestResult, TestStatus};

/// Parse Jest JSON output into a list of test results.
///
/// Supports both the `testResults` and `assertionResults` field names
/// for individual test entries within each file result.
///
/// # Errors
///
/// Returns `T3strError::ParseFailed` if the input is not valid JSON
/// or does not contain the expected structure.
pub fn parse(json: &str) -> super::ParseResult {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| T3strError::ParseFailed {
            format: "jest-json".to_owned(),
            reason: e.to_string(),
        })?;

    let file_results = root
        .get("testResults")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| T3strError::ParseFailed {
            format: "jest-json".to_owned(),
            reason: "missing top-level testResults array".to_owned(),
        })?;

    let mut results = Vec::new();

    for file_entry in file_results {
        let file_path: Option<String> = file_entry
            .get("testFilePath")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);

        // Try "testResults" first, then "assertionResults"
        let assertions: Option<&Vec<serde_json::Value>> = file_entry
            .get("testResults")
            .and_then(serde_json::Value::as_array)
            .or_else(|| {
                file_entry
                    .get("assertionResults")
                    .and_then(serde_json::Value::as_array)
            });

        let Some(assertions) = assertions else {
            continue;
        };

        for assertion in assertions {
            let title: &str = match assertion.get("title").and_then(serde_json::Value::as_str) {
                Some(t) => t,
                None => continue,
            };

            let status_str = assertion
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();

            let status = match status_str {
                "passed" => TestStatus::Passed,
                "failed" => TestStatus::Failed,
                "pending" | "skipped" => TestStatus::Skipped,
                _ => continue,
            };

            let duration_ms = assertion
                .get("duration")
                .and_then(serde_json::Value::as_u64);

            let message = assertion
                .get("failureMessages")
                .and_then(serde_json::Value::as_array)
                .and_then(|msgs: &Vec<serde_json::Value>| {
                    let joined: String = msgs
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if joined.is_empty() {
                        None
                    } else {
                        Some(joined)
                    }
                });

            results.push(TestResult {
                name: title.to_owned(),
                status,
                duration_ms,
                message,
                file: file_path.clone(),
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
#[path = "jest_json_tests.rs"]
mod tests;
