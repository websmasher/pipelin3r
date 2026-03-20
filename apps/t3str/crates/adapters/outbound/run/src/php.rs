//! PHP test execution and output parsing.
//!
//! Detects test framework (Nette Tester or `PHPUnit`) and executes accordingly.
//! Nette Tester output is parsed as text, `PHPUnit` produces `JUnit` XML.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::{junit_xml, nette_text};

/// Timeout for PHP test execution in seconds.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute PHP tests in the given directory.
///
/// Detects the test framework by checking for `vendor/bin/tester` (Nette Tester)
/// first, then `vendor/bin/phpunit`, and finally a global `phpunit` binary.
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] if the test command times out,
/// or [`T3strError::Io`] if the process cannot be spawned.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let nette_tester = repo_dir.join("vendor/bin/tester");
    let local_phpunit = repo_dir.join("vendor/bin/phpunit");

    let (results, combined_output) = if nette_tester.exists() {
        run_nette_tester(&nette_tester, repo_dir).await?
    } else {
        let phpunit = if local_phpunit.exists() {
            local_phpunit.to_string_lossy().into_owned()
        } else {
            String::from("phpunit")
        };
        run_phpunit(&phpunit, repo_dir, filter).await?
    };

    let summary = build_summary(&results);
    Ok(TestSuite {
        language: Language::Php,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined_output, RAW_OUTPUT_MAX)),
    })
}

/// Run Nette Tester and parse console output.
async fn run_nette_tester(
    tester_path: &Path,
    repo_dir: &Path,
) -> Result<(Vec<t3str_domain_types::TestResult>, String), T3strError> {
    let tester_str = tester_path.to_string_lossy();
    let (stdout, stderr, _code) = run_command(
        tester_str.as_ref(),
        &["tests/", "-o", "console"],
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Php,
    )
    .await?;

    let mut output = stdout.clone();
    output.push('\n');
    output.push_str(&stderr);

    let results = nette_text::parse(&stdout);
    Ok((results, output))
}

/// Common test directory names to try when `PHPUnit` finds no tests.
const TEST_DIRS: [&str; 3] = ["tests", "test", "Test"];

/// Run `PHPUnit` with `JUnit` XML output and parse.
///
/// If the initial run produces no results (no XML or empty XML), retries
/// with common test directory arguments. If `--log-junit` is unsupported
/// (XML file never created), falls back to parsing counts from stdout.
async fn run_phpunit(
    phpunit_bin: &str,
    repo_dir: &Path,
    filter: Option<&str>,
) -> Result<(Vec<t3str_domain_types::TestResult>, String), T3strError> {
    let output_file = repo_dir.join("test-output.xml");
    let output_path = output_file.to_string_lossy().into_owned();

    let log_arg = format!("--log-junit={output_path}");
    let mut args: Vec<String> = vec![log_arg.clone()];
    if let Some(f) = filter {
        args.push(String::from("--filter"));
        args.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        phpunit_bin,
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Php,
    )
    .await?;

    let mut output = stdout;
    output.push('\n');
    output.push_str(&stderr);

    // Parse XML if available, fall back to empty results.
    let mut results = if output_file.exists() {
        let xml = tokio::fs::read_to_string(&output_file)
            .await
            .map_err(T3strError::Io)?;
        junit_xml::parse(&xml).unwrap_or_default()
    } else {
        Vec::new()
    };

    let xml_existed = output_file.exists();

    // Fix 1: If no results, retry with common test directory arguments.
    if results.is_empty() {
        for dir_name in &TEST_DIRS {
            if repo_dir.join(dir_name).is_dir() {
                let (retry_results, retry_output) = run_phpunit_with_dir(
                    phpunit_bin,
                    repo_dir,
                    filter,
                    dir_name,
                    &output_file,
                    &log_arg,
                )
                .await?;
                output.push('\n');
                output.push_str(&retry_output);
                if !retry_results.is_empty() {
                    results = retry_results;
                    break;
                }
            }
        }
    }

    // Fix 2: If XML was never created (--log-junit unsupported), parse stdout.
    if results.is_empty() && !xml_existed {
        results = parse_phpunit_text_output(&output);
    }

    // Fix 3: If still empty and output indicates a compatibility error, report it
    // instead of silently returning zero results.
    if results.is_empty() {
        if let Some(error_result) = detect_compatibility_error(&output) {
            results.push(error_result);
        }
    }

    Ok((results, output))
}

/// Re-run `PHPUnit` with an explicit test directory argument.
async fn run_phpunit_with_dir(
    phpunit_bin: &str,
    repo_dir: &Path,
    filter: Option<&str>,
    test_dir: &str,
    output_file: &Path,
    log_arg: &str,
) -> Result<(Vec<t3str_domain_types::TestResult>, String), T3strError> {
    let mut args: Vec<String> = vec![String::from(log_arg), String::from(test_dir)];
    if let Some(f) = filter {
        args.push(String::from("--filter"));
        args.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        phpunit_bin,
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Php,
    )
    .await?;

    let mut output = stdout;
    output.push('\n');
    output.push_str(&stderr);

    let results = if output_file.exists() {
        let xml = tokio::fs::read_to_string(output_file)
            .await
            .map_err(T3strError::Io)?;
        junit_xml::parse(&xml).unwrap_or_default()
    } else {
        // --log-junit not supported; parse text output instead.
        parse_phpunit_text_output(&output)
    };

    Ok((results, output))
}

/// Parse `PHPUnit` text output to extract a summary test result.
///
/// Looks for patterns like `OK (5 tests, 10 assertions)` or
/// `Tests: 5, Assertions: 10, Failures: 2`.
fn parse_phpunit_text_output(output: &str) -> Vec<t3str_domain_types::TestResult> {
    // Look for "OK (N test" pattern — all tests passed.
    if let Some(count) = extract_ok_count(output) {
        if count > 0 {
            return vec![t3str_domain_types::TestResult {
                name: String::from("PHPUnit suite (from text output)"),
                status: t3str_domain_types::TestStatus::Passed,
                duration_ms: None,
                message: Some(format!("{count} test(s) passed")),
                file: None,
            }];
        }
    }

    // Look for "Tests: N, Assertions:" pattern — may include failures.
    if let Some((total, failures)) = extract_summary_counts(output) {
        if total > 0 {
            let status = if failures > 0 {
                t3str_domain_types::TestStatus::Failed
            } else {
                t3str_domain_types::TestStatus::Passed
            };
            return vec![t3str_domain_types::TestResult {
                name: String::from("PHPUnit suite (from text output)"),
                status,
                duration_ms: None,
                message: Some(format!(
                    "{total} test(s), {failures} failure(s)"
                )),
                file: None,
            }];
        }
    }

    Vec::new()
}

/// Detect known compatibility errors in `PHPUnit` output and create an error result.
///
/// When `PHPUnit` 11 encounters test files using the legacy `PHPUnit_Framework_TestCase`
/// class (`PHPUnit` 3.x/4.x API), it crashes with a fatal error. This function detects
/// that situation and returns a descriptive error result instead of silent empty results.
fn detect_compatibility_error(output: &str) -> Option<t3str_domain_types::TestResult> {
    let is_legacy_api = output.contains("PHPUnit_Framework_TestCase")
        || output.contains("PHPUnit_Framework_Test");
    let has_fatal = output.contains("Fatal error:");

    if is_legacy_api || has_fatal {
        let message = if is_legacy_api {
            "Test uses legacy PHPUnit API (PHPUnit_Framework_TestCase) \
             incompatible with PHPUnit 11"
        } else {
            "PHPUnit execution failed with a fatal error"
        };
        Some(t3str_domain_types::TestResult {
            name: String::from("PHPUnit compatibility"),
            status: t3str_domain_types::TestStatus::Error,
            duration_ms: None,
            message: Some(String::from(message)),
            file: None,
        })
    } else {
        None
    }
}

/// Extract the test count from `OK (N test` in `PHPUnit` output.
fn extract_ok_count(output: &str) -> Option<u32> {
    // Looking for "OK (N test" where N is one or more digits.
    let marker = "OK (";
    let pos = output.find(marker)?;
    let after = output.get(pos.saturating_add(marker.len())..)?;
    let digit_end = after.find(|c: char| !c.is_ascii_digit())?;
    let digits = after.get(..digit_end)?;
    digits.parse::<u32>().ok()
}

/// Total tests and failure count from `PHPUnit` summary output.
type SummaryCounts = (u32, u32);

/// Extract total and failure counts from `Tests: N, Assertions: M` lines.
///
/// Returns `(total_tests, failures)`.
fn extract_summary_counts(output: &str) -> Option<SummaryCounts> {
    // Looking for "Tests: N," pattern.
    let tests_marker = "Tests: ";
    let pos = output.find(tests_marker)?;
    let after_tests = output.get(pos.saturating_add(tests_marker.len())..)?;
    let digit_end = after_tests.find(|c: char| !c.is_ascii_digit())?;
    let total_str = after_tests.get(..digit_end)?;
    let total = total_str.parse::<u32>().ok()?;

    // Look for "Failures: N" in the same line region.
    let failures_marker = "Failures: ";
    let failures = if let Some(fpos) = output.get(pos..)?.find(failures_marker) {
        let after_f = output.get(
            pos.checked_add(fpos)?.checked_add(failures_marker.len())?..
        )?;
        let f_end = after_f
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_f.len());
        let f_str = after_f.get(..f_end)?;
        f_str.parse::<u32>().ok().unwrap_or(0)
    } else {
        0
    };

    Some((total, failures))
}
