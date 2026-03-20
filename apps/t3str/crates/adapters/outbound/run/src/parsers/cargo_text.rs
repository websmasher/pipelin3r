//! Parser for `cargo test` text output.
//!
//! Parses the human-readable output of `cargo test` into [`TestResult`] values.
//! Uses only `str` methods — no regex.

use t3str_domain_types::{TestResult, TestStatus};

/// Parse `cargo test` text output into test results.
///
/// Processes line by line, matching lines that start with `"test "` and contain
/// the `" ... "` separator. Status is determined by the token after the separator:
/// `"ok"` → [`TestStatus::Passed`], `"FAILED"` → [`TestStatus::Failed`],
/// `"ignored"` → [`TestStatus::Skipped`].
///
/// Returns an empty `Vec` if no test lines are found (lenient parsing).
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Match lines starting with "test " (with trailing space).
        let Some(rest) = trimmed.strip_prefix("test ") else {
            continue;
        };

        // Find the " ... " separator.
        let separator = " ... ";
        let Some(sep_pos) = rest.find(separator) else {
            continue;
        };

        let name = rest.get(..sep_pos).unwrap_or_default().trim();
        let status_str = rest
            .get(sep_pos.saturating_add(separator.len())..)
            .unwrap_or_default()
            .trim();

        let status = match status_str {
            "ok" => TestStatus::Passed,
            "FAILED" => TestStatus::Failed,
            "ignored" => TestStatus::Skipped,
            _ => continue,
        };

        results.push(TestResult {
            name: name.to_owned(),
            status,
            duration_ms: None,
            message: None,
            file: None,
        });
    }

    results
}

#[cfg(test)]
#[path = "cargo_text_tests.rs"]
mod tests;
