//! Parser for Mocha text output.
//!
//! Parses the human-readable output of Mocha (JavaScript test framework) into
//! [`TestResult`] values. Uses only `str` methods — no regex.

use t3str_domain_types::{TestResult, TestStatus};

/// Checkmark characters that Mocha uses for passing tests.
const PASS_MARKERS: &[char] = &[
    '\u{2713}', // ✓
    '\u{2714}', // ✔
    '\u{221A}', // √
];

/// Parse Mocha text output into test results.
///
/// Passing tests are identified by a checkmark character (`✓`, `✔`, or `√`).
/// Failing tests are identified by a numbered prefix like `"1) "`.
/// Duration is extracted from a trailing `(Nms)` suffix on passing tests.
///
/// Returns an empty `Vec` if no test lines are found (lenient parsing).
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(result) = try_parse_passing(trimmed) {
            results.push(result);
        } else if let Some(result) = try_parse_failing(trimmed) {
            results.push(result);
        }
    }

    results
}

/// Try to parse a passing test line (contains a checkmark).
fn try_parse_passing(trimmed: &str) -> Option<TestResult> {
    // Find the first checkmark character.
    let marker_pos = trimmed.find(PASS_MARKERS)?;

    // Everything after the checkmark is the test name (+ optional duration).
    let after_marker = trimmed
        .get(marker_pos.saturating_add(checkmark_len(trimmed, marker_pos))..)?
        .trim();

    if after_marker.is_empty() {
        return None;
    }

    let (name, duration_ms) = extract_name_and_duration(after_marker);

    if name.is_empty() {
        return None;
    }

    Some(TestResult {
        name: name.to_owned(),
        status: TestStatus::Passed,
        duration_ms,
        message: None,
        file: None,
    })
}

/// Get the byte length of the checkmark character at the given byte position.
fn checkmark_len(s: &str, byte_pos: usize) -> usize {
    s.get(byte_pos..)
        .and_then(|sub| sub.chars().next())
        .map_or(1, char::len_utf8)
}

/// Try to parse a failing test line (starts with `N) ` pattern).
fn try_parse_failing(trimmed: &str) -> Option<TestResult> {
    // Find the first ')' character.
    let paren_pos = trimmed.find(')')?;

    // Everything before ')' must be a number.
    let number_part = trimmed.get(..paren_pos)?;
    if number_part.is_empty() || !number_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // After ") " is the test name.
    let after_paren = trimmed.get(paren_pos.saturating_add(1)..)?.trim();

    if after_paren.is_empty() {
        return None;
    }

    // Skip summary lines like "2 passing" or "1 failing".
    if after_paren.contains(" passing") || after_paren.contains(" failing") {
        return None;
    }

    Some(TestResult {
        name: after_paren.to_owned(),
        status: TestStatus::Failed,
        duration_ms: None,
        message: None,
        file: None,
    })
}

/// A test name paired with an optional duration in milliseconds.
type NameAndDuration<'a> = (&'a str, Option<u64>);

/// Extract test name and optional `(Nms)` duration from a string.
fn extract_name_and_duration(s: &str) -> NameAndDuration<'_> {
    // Look for trailing "(Nms)" pattern.
    if let Some(paren_start) = s.rfind('(') {
        if let Some(before_paren) = s.get(..paren_start) {
            let inside = s
                .get(paren_start.saturating_add(1)..)
                .and_then(|p| p.strip_suffix(')'))
                .unwrap_or_default()
                .trim();

            if let Some(ms_val) = parse_ms_duration(inside) {
                let name = before_paren.trim();
                if !name.is_empty() {
                    return (name, Some(ms_val));
                }
            }
        }
    }

    (s.trim(), None)
}

/// Parse a duration string like `"45ms"` into milliseconds.
fn parse_ms_duration(s: &str) -> Option<u64> {
    let ms_part = s.strip_suffix("ms")?;
    ms_part.trim().parse::<u64>().ok()
}

#[cfg(test)]
#[path = "mocha_text_tests.rs"]
mod tests;
