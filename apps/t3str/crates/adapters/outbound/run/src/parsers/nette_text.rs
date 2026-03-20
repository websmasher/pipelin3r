//! Parser for Nette Tester console output (`-o console`).
//!
//! Parses the human-readable output of Nette Tester (PHP) into [`TestResult`]
//! values. Uses only `str` methods — no regex.
//!
//! The console output format uses Unicode markers per test:
//! - `√ testName` — passed
//! - `× testName` — failed
//! - `s testName` or `s testName (reason)` — skipped
//!
//! Failure details appear as `Failed: message in testName()` lines below the
//! test list. A summary line like `FAILURES! (37 tests, 3 failures, 2.6 seconds)`
//! or `OK (37 tests, 2.6 seconds)` may appear at the end.

use t3str_domain_types::{TestResult, TestStatus};

/// A failure detail: test name and the failure message.
type FailureDetail = (String, String);

/// Marker for a passed test in Nette Tester console output.
const PASS_MARKER: &str = "√ ";

/// Marker for a failed test in Nette Tester console output.
const FAIL_MARKER: &str = "× ";

/// Marker for a skipped test in Nette Tester console output.
const SKIP_MARKER: &str = "s ";

/// Prefix for failure detail lines.
const FAILED_PREFIX: &str = "Failed: ";

/// Parse Nette Tester console output into test results.
///
/// Matches lines starting with `"√ "` (passed), `"× "` (failed), or
/// `"s "` (skipped). For skipped tests, a trailing parenthetical reason is
/// stripped from the test name.
///
/// Failure detail lines starting with `"Failed: "` are collected and attached
/// as messages to matching failed test results.
///
/// Returns an empty `Vec` if no matching lines are found (lenient parsing).
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();
    let mut failure_messages: Vec<FailureDetail> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(name) = trimmed.strip_prefix(PASS_MARKER) {
            let name = name.trim();
            if !name.is_empty() {
                results.push(TestResult {
                    name: name.to_owned(),
                    status: TestStatus::Passed,
                    duration_ms: None,
                    message: None,
                    file: None,
                });
            }
        } else if let Some(name) = trimmed.strip_prefix(FAIL_MARKER) {
            let name = name.trim();
            if !name.is_empty() {
                results.push(TestResult {
                    name: name.to_owned(),
                    status: TestStatus::Failed,
                    duration_ms: None,
                    message: None,
                    file: None,
                });
            }
        } else if let Some(rest) = trimmed.strip_prefix(SKIP_MARKER) {
            let name = strip_trailing_parenthetical(rest);
            if !name.is_empty() {
                results.push(TestResult {
                    name: name.to_owned(),
                    status: TestStatus::Skipped,
                    duration_ms: None,
                    message: None,
                    file: None,
                });
            }
        } else if let Some(detail) = trimmed.strip_prefix(FAILED_PREFIX) {
            // Lines like: "Failed: true should be false in testParseStringSignedFile()"
            // Extract the test name from "... in testName()"
            if let Some(msg_and_name) = extract_failure_detail(detail) {
                failure_messages.push(msg_and_name);
            }
        }
    }

    // Attach failure messages to matching failed results.
    for (test_name, message) in &failure_messages {
        for result in &mut results {
            if result.status == TestStatus::Failed && result.name == *test_name {
                result.message = Some(message.clone());
            }
        }
    }

    results
}

/// Extract test name and message from a failure detail line.
///
/// Input: `"true should be false in testParseStringSignedFile()"`
/// Returns: `Some(("testParseStringSignedFile", "true should be false"))`
fn extract_failure_detail(detail: &str) -> Option<FailureDetail> {
    let trimmed = detail.trim();

    // Look for " in " followed by a function name ending with "()"
    let marker = " in ";
    let in_pos = trimmed.rfind(marker)?;

    let message = trimmed.get(..in_pos)?.trim();
    let after_in = trimmed.get(in_pos.saturating_add(marker.len())..)?;

    // Strip trailing "()" from the test name.
    let test_name = after_in.trim().strip_suffix("()")?.trim();

    if test_name.is_empty() || message.is_empty() {
        return None;
    }

    Some((test_name.to_owned(), message.to_owned()))
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
