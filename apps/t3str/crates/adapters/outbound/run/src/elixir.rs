//! Elixir test execution and output parsing.
//!
//! Runs `mix test` and parses the human-readable output for failure details
//! and summary counts.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::mix_text;

/// Timeout for Elixir test execution in seconds.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute Elixir tests in the given directory.
///
/// Runs `mix test` and parses the output for named failures (via
/// [`mix_text::parse`]) and summary counts (via [`mix_text::parse_summary`]).
/// If a filter is provided, appends `--only <filter>`.
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] if the test command times out,
/// or [`T3strError::Io`] if the process cannot be spawned.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let mut args_owned: Vec<String> = vec![String::from("test")];
    if let Some(f) = filter {
        args_owned.push(String::from("--only"));
        args_owned.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args_owned.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        "mix",
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Elixir,
    )
    .await?;

    let mut combined = stdout;
    combined.push('\n');
    combined.push_str(&stderr);

    // Parse named failures from the output.
    let results = mix_text::parse(&combined);

    // Build summary from parse_summary if available, otherwise from results.
    let summary = if let Some((total, failures, skipped)) = mix_text::parse_summary(&combined) {
        let passed = total.saturating_sub(failures).saturating_sub(skipped);
        t3str_domain_types::TestSummary {
            total,
            passed,
            failed: failures,
            skipped,
            errors: 0,
        }
    } else {
        build_summary(&results)
    };

    Ok(TestSuite {
        language: Language::Elixir,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}
