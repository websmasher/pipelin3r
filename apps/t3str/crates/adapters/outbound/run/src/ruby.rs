//! Ruby test execution and output parsing.
//!
//! Tries `RSpec` (with JSON format) first, falls back to rake test,
//! then falls back to running minitest files directly without bundler.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::rspec_json;

/// Timeout for Ruby test execution in seconds.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute Ruby tests in the given directory.
///
/// Sets `BUNDLE_PATH=.bundle` and attempts `RSpec` with JSON output first.
/// If `RSpec` produces no structured results and exits non-zero, falls back
/// to `bundle exec rake test`.
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] if the test command times out,
/// or [`T3strError::Io`] if the process cannot be spawned.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let env: &[crate::helpers::EnvVar<'_>] = &[("BUNDLE_PATH", ".bundle")];

    let output_file = repo_dir.join("test-output.json");
    let output_path = output_file.to_string_lossy().into_owned();

    let mut args_owned: Vec<String> = vec![
        String::from("exec"),
        String::from("rspec"),
        String::from("--format"),
        String::from("json"),
        String::from("--out"),
        output_path,
    ];
    if let Some(f) = filter {
        args_owned.push(String::from("--tag"));
        args_owned.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args_owned.iter().map(String::as_str).collect();
    let (stdout, stderr, exit_code) = run_command(
        "bundle",
        &arg_refs,
        repo_dir,
        env,
        TIMEOUT_SECS,
        Language::Ruby,
    )
    .await?;

    let mut combined = stdout;
    combined.push('\n');
    combined.push_str(&stderr);

    // Try parsing JSON output file from RSpec.
    let mut results = if output_file.exists() {
        let json = tokio::fs::read_to_string(&output_file)
            .await
            .map_err(T3strError::Io)?;
        rspec_json::parse(&json).unwrap_or_default()
    } else {
        Vec::new()
    };

    // If RSpec failed with no structured results, fall back to rake test.
    if results.is_empty() && exit_code != 0 {
        let (stdout2, stderr2, _) = run_command(
            "bundle",
            &["exec", "rake", "test"],
            repo_dir,
            env,
            TIMEOUT_SECS,
            Language::Ruby,
        )
        .await?;
        combined.push('\n');
        combined.push_str(&stdout2);
        combined.push('\n');
        combined.push_str(&stderr2);
        // No structured output from rake — results stay empty.
    }

    // If bundler-based approaches failed, try running minitest files directly.
    if results.is_empty() {
        let (minitest_results, minitest_output) = run_minitest_direct(repo_dir).await;
        combined.push('\n');
        combined.push_str(&minitest_output);
        if !minitest_results.is_empty() {
            results = minitest_results;
        }
    }

    let summary = build_summary(&results);
    Ok(TestSuite {
        language: Language::Ruby,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}

/// Run minitest test files directly with `ruby -Ilib -Itest`, bypassing bundler.
///
/// This works when the library has no complex dependencies beyond Ruby stdlib
/// and minitest (which ships with Ruby). Useful when bundler cannot install
/// gems (e.g., Rails 5.1 on Ruby 3.1) but the core library tests are standalone.
async fn run_minitest_direct(
    repo_dir: &Path,
) -> (Vec<t3str_domain_types::TestResult>, String) {
    let script = concat!(
        "require 'minitest/autorun'; ",
        "Dir['test/**/*_test.rb']",
        ".reject{|f| f.include?('dummy') || f.include?('.bundle') || f.include?('vendor')}",
        ".sort",
        ".each{|f| begin; load f; rescue LoadError, NameError => e; ",
        "$stderr.puts \"skip #{f}: #{e.message}\"; end}",
    );

    let result = run_command(
        "ruby",
        &["-Ilib", "-Itest", "-e", script],
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Ruby,
    )
    .await;

    let Ok((stdout, stderr, _exit_code)) = result else {
        return (Vec::new(), String::new());
    };

    let mut output = stdout;
    output.push('\n');
    output.push_str(&stderr);

    let results = parse_minitest_output(&output);
    (results, output)
}

/// Parse minitest summary output into test results.
///
/// Minitest prints a summary line like:
/// `5 runs, 10 assertions, 0 failures, 0 errors, 0 skips`
fn parse_minitest_output(output: &str) -> Vec<t3str_domain_types::TestResult> {
    // Look for "N runs, N assertions, N failures, N errors" pattern.
    let marker = " runs, ";
    let Some(marker_pos) = output.find(marker) else {
        return Vec::new();
    };

    // Extract the runs count (digits before " runs, ").
    let Some(before_marker) = output.get(..marker_pos) else {
        return Vec::new();
    };
    let runs_start = before_marker
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |p| p.saturating_add(1));
    let Some(runs_str) = before_marker.get(runs_start..) else {
        return Vec::new();
    };
    let Ok(runs) = runs_str.parse::<u32>() else {
        return Vec::new();
    };

    if runs == 0 {
        return Vec::new();
    }

    // Extract failures count from "N failures".
    let failures = extract_count_before(output, " failures,")
        .or_else(|| extract_count_before(output, " failures"))
        .unwrap_or(0);

    // Extract errors count from "N errors".
    let errors = extract_count_before(output, " errors,")
        .or_else(|| extract_count_before(output, " errors"))
        .unwrap_or(0);

    let total_bad = failures.saturating_add(errors);
    let status = if total_bad > 0 {
        t3str_domain_types::TestStatus::Failed
    } else {
        t3str_domain_types::TestStatus::Passed
    };

    vec![t3str_domain_types::TestResult {
        name: String::from("minitest suite (direct run)"),
        status,
        duration_ms: None,
        message: Some(format!(
            "{runs} run(s), {failures} failure(s), {errors} error(s)"
        )),
        file: None,
    }]
}

/// Extract a numeric count that appears immediately before the given marker
/// string.
fn extract_count_before(output: &str, marker: &str) -> Option<u32> {
    let pos = output.find(marker)?;
    let before = output.get(..pos)?;
    // Walk backwards to find the start of the digit sequence.
    let start = before
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |p| p.saturating_add(1));
    let digits = before.get(start..)?;
    digits.parse::<u32>().ok()
}
