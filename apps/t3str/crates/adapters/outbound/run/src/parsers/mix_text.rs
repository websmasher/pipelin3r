//! Parser for Elixir `mix test` output.
//!
//! Parses the human-readable output of `mix test` into [`TestResult`] values
//! and summary counts. Uses only `str` methods — no regex.
//!
//! Since `mix test` only names tests in the failure section, passing tests
//! cannot be individually identified from the default output. This parser
//! returns named [`TestResult`] entries for failures and extracts summary
//! counts via [`parse_summary`].

use t3str_domain_types::{TestResult, TestStatus};

/// Parse `mix test` output into test results.
///
/// Extracts named failed tests from the failure section. Lines matching the
/// pattern `N) test {name} ({module})` produce [`TestStatus::Failed`] entries.
///
/// Passing and skipped tests are not individually named in mix output, so
/// use [`parse_summary`] to get aggregate counts.
///
/// Returns an empty `Vec` if no failure entries are found (lenient parsing).
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(result) = try_parse_failure_entry(trimmed) {
            results.push(result);
        }
    }

    results
}

/// Extract summary counts from `mix test` output.
///
/// Looks for a line matching `"N tests, N failure(s)"` with optional
/// `", N skipped"` suffix. Returns `(total, failures, skipped)`.
///
/// Summary counts: `(total, failures, skipped)`.
pub type MixSummary = (u32, u32, u32);

/// Returns `None` if no summary line is found.
pub fn parse_summary(output: &str) -> Option<MixSummary> {
    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(counts) = try_parse_summary_line(trimmed) {
            return Some(counts);
        }
    }

    None
}

/// Try to parse a failure entry line like `"1) test something (MyModule)"`.
fn try_parse_failure_entry(trimmed: &str) -> Option<TestResult> {
    // Find the closing paren of the number prefix: "N) test "
    let paren_pos = trimmed.find(')')?;

    // Everything before ')' must be digits.
    let number_part = trimmed.get(..paren_pos)?;
    if number_part.is_empty() || !number_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // After ") " must start with "test ".
    let after_paren = trimmed.get(paren_pos.saturating_add(1)..)?.trim();
    let after_test = after_paren.strip_prefix("test ")?;

    if after_test.is_empty() {
        return None;
    }

    // Extract module from trailing "(Module)" if present.
    let (test_name, module) = extract_module(after_test);

    let full_name = if let Some(m) = module {
        let mut name =
            String::with_capacity(m.len().saturating_add(test_name.len()).saturating_add(1));
        name.push_str(m);
        name.push('.');
        name.push_str(test_name);
        name
    } else {
        test_name.to_owned()
    };

    Some(TestResult {
        name: full_name,
        status: TestStatus::Failed,
        duration_ms: None,
        message: None,
        file: None,
    })
}

/// Extract a trailing `(Module)` from a test name string.
///
/// A test name paired with an optional module name.
type NameAndModule<'a> = (&'a str, Option<&'a str>);

/// Returns `(name, Some(module))` if a parenthetical module is found,
/// or `(original, None)` otherwise.
fn extract_module(s: &str) -> NameAndModule<'_> {
    let trimmed = s.trim();
    if trimmed.ends_with(')') {
        if let Some(paren_start) = trimmed.rfind('(') {
            let name = trimmed.get(..paren_start).unwrap_or(trimmed).trim();
            let module = trimmed
                .get(paren_start.saturating_add(1)..trimmed.len().saturating_sub(1))
                .unwrap_or_default()
                .trim();
            if !module.is_empty() {
                return (name, Some(module));
            }
        }
    }
    (trimmed, None)
}

/// Try to parse a summary line like `"4 tests, 1 failure, 1 skipped"`.
fn try_parse_summary_line(trimmed: &str) -> Option<MixSummary> {
    // Must contain " test" (handles "tests" and "test").
    if !trimmed.contains(" test") {
        return None;
    }

    // Must contain "failure" (handles "failure" and "failures").
    if !trimmed.contains("failure") {
        return None;
    }

    // Split on commas and parse each segment.
    let mut total: Option<u32> = None;
    let mut failures: Option<u32> = None;
    let mut skipped: u32 = 0;

    for segment in trimmed.split(',') {
        let seg = segment.trim();

        if seg.contains("test") {
            total = extract_leading_number(seg);
        } else if seg.contains("failure") {
            failures = extract_leading_number(seg);
        } else if seg.contains("skipped") {
            skipped = extract_leading_number(seg).unwrap_or(0);
        }
    }

    Some((total?, failures?, skipped))
}

/// Extract the leading number from a string like `"4 tests"` → `4`.
fn extract_leading_number(s: &str) -> Option<u32> {
    let trimmed = s.trim();
    let end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let num_str = trimmed.get(..end)?;
    num_str.parse::<u32>().ok()
}

#[cfg(test)]
#[path = "mix_text_tests.rs"]
mod tests;
