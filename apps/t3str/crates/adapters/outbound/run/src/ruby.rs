//! Ruby test execution and output parsing.
//!
//! Tries `RSpec` (with JSON format) first, falls back to rake test.

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
    let results = if output_file.exists() {
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

    let summary = build_summary(&results);
    Ok(TestSuite {
        language: Language::Ruby,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}
