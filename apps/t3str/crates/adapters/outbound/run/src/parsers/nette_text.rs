//! Parser for Nette Tester console output.
//!
//! Parses the human-readable output of Nette Tester (PHP) into [`TestResult`]
//! values. Uses only `str` methods — no regex.

use t3str_domain_types::{TestResult, TestStatus};

/// Parse Nette Tester console output into test results.
///
/// Matches lines starting with `"-- PASSED: "`, `"-- FAILED: "`, or
/// `"-- SKIPPED: "`. For skipped tests, a trailing parenthetical reason is
/// stripped from the test name.
///
/// Returns an empty `Vec` if no matching lines are found (lenient parsing).
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        let (status, rest) = if let Some(r) = trimmed.strip_prefix("-- PASSED: ") {
            (TestStatus::Passed, r)
        } else if let Some(r) = trimmed.strip_prefix("-- FAILED: ") {
            (TestStatus::Failed, r)
        } else if let Some(r) = trimmed.strip_prefix("-- SKIPPED: ") {
            (TestStatus::Skipped, r)
        } else {
            continue;
        };

        let name = if status == TestStatus::Skipped {
            strip_trailing_parenthetical(rest)
        } else {
            rest.trim()
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

/// Strip a trailing `(reason)` parenthetical from a string.
///
/// If the string ends with `)` and contains a `(`, everything from the last
/// `(` onward is removed and the remainder is trimmed.
fn strip_trailing_parenthetical(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.ends_with(')') {
        if let Some(paren_start) = trimmed.rfind('(') {
            return trimmed.get(..paren_start).unwrap_or(trimmed).trim();
        }
    }
    trimmed
}

#[cfg(test)]
#[path = "nette_text_tests.rs"]
mod tests;
