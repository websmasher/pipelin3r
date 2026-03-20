//! Parser for `dotnet test` verbose stdout output.
//!
//! Parses the human-readable verbose output of `dotnet test` into [`TestResult`]
//! values. Uses only `str` methods — no regex.

use t3str_domain_types::{TestResult, TestStatus};

use super::seconds_to_ms;

/// Parse `dotnet test` verbose stdout into test results.
///
/// Processes line by line, matching trimmed lines starting with `"Passed "`,
/// `"Failed "`, or `"Skipped "`. Duration is extracted from a trailing
/// `[N ms]` or `[N s]` bracket. `[< 1 ms]` is treated as 0 ms.
///
/// Returns an empty `Vec` if no matching lines are found (lenient parsing).
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        let (status, rest) = if let Some(r) = trimmed.strip_prefix("Passed ") {
            (TestStatus::Passed, r)
        } else if let Some(r) = trimmed.strip_prefix("Failed ") {
            (TestStatus::Failed, r)
        } else if let Some(r) = trimmed.strip_prefix("Skipped ") {
            (TestStatus::Skipped, r)
        } else {
            continue;
        };

        let (name, duration_ms) = extract_name_and_duration(rest);

        results.push(TestResult {
            name: name.to_owned(),
            status,
            duration_ms,
            message: None,
            file: None,
        });
    }

    results
}

/// A test name paired with an optional duration in milliseconds.
type NameAndDuration<'a> = (&'a str, Option<u64>);

/// Extract test name and optional duration from the remainder after the status word.
///
/// The duration appears as `[N ms]`, `[< 1 ms]`, or `[N.NNN s]` at the end.
fn extract_name_and_duration(rest: &str) -> NameAndDuration<'_> {
    // Look for the last " [" which starts the duration bracket.
    let bracket_start = rest.rfind(" [");
    let bracket_end = rest.rfind(']');

    match (bracket_start, bracket_end) {
        (Some(start), Some(end)) if end > start => {
            let name = rest.get(..start).unwrap_or(rest).trim();
            let duration_str = rest
                .get(start.saturating_add(2)..end)
                .unwrap_or_default()
                .trim();
            let duration_ms = parse_duration(duration_str);
            (name, duration_ms)
        }
        _ => (rest.trim(), None),
    }
}

/// Parse a duration string like `"5 ms"`, `"< 1 ms"`, or `"1.234 s"`.
fn parse_duration(s: &str) -> Option<u64> {
    let trimmed = s.trim();

    // Handle "< 1 ms" → 0
    if trimmed.starts_with("< ") {
        return Some(0);
    }

    if let Some(ms_part) = trimmed.strip_suffix(" ms") {
        // Parse integer milliseconds.
        ms_part.trim().parse::<u64>().ok()
    } else if let Some(s_part) = trimmed.strip_suffix(" s") {
        // Parse seconds as f64, convert to ms.
        let secs: f64 = s_part.trim().parse().ok()?;
        seconds_to_ms(secs)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "dotnet_stdout_tests.rs"]
mod tests;
