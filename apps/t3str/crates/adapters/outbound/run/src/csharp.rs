//! C# test execution and output parsing.
//!
//! Runs `dotnet test` with verbose output and parses the stdout line-by-line
//! to extract individual test results.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::dotnet_stdout;

/// Timeout for C# test execution in seconds (builds can be slow).
const TIMEOUT_SECS: u64 = 600;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute C# tests in the given directory.
///
/// Runs `dotnet test --verbosity normal -m:1` with telemetry and globalization
/// disabled. Parses the verbose stdout to extract test results.
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] if the test command times out,
/// or [`T3strError::Io`] if the process cannot be spawned.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let env_vars: &[crate::helpers::EnvVar<'_>] = &[
        ("DOTNET_CLI_TELEMETRY_OPTOUT", "1"),
        ("DOTNET_SYSTEM_GLOBALIZATION_INVARIANT", "1"),
        ("MSBUILDDISABLENODEREUSE", "1"),
    ];

    let mut args: Vec<String> = vec![
        String::from("test"),
        String::from("--verbosity"),
        String::from("normal"),
        String::from("-m:1"),
    ];
    if let Some(f) = filter {
        args.push(String::from("--filter"));
        args.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        "dotnet",
        &arg_refs,
        repo_dir,
        env_vars,
        TIMEOUT_SECS,
        Language::Csharp,
    )
    .await?;

    let mut combined = stdout.clone();
    combined.push('\n');
    combined.push_str(&stderr);

    let results = dotnet_stdout::parse(&stdout);
    let summary = build_summary(&results);

    Ok(TestSuite {
        language: Language::Csharp,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}
