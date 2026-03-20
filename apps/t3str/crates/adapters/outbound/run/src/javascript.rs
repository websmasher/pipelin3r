//! JavaScript/TypeScript test execution and output parsing.
//!
//! Runs `npm test` to capture whatever test framework is configured, then
//! attempts `npx jest --json` for structured output. Parses results using
//! Jest JSON or Mocha text parsers.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestResult, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::{jest_json, mocha_text};

/// Timeout for JavaScript test execution in seconds.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute JavaScript/TypeScript tests in the given directory.
///
/// First runs `npm test` to invoke the project's configured test script.
/// Then attempts `npx jest --json` for structured output. Results are parsed
/// with the Jest JSON parser if structured output is available, otherwise
/// falls back to Mocha text parsing of npm test stdout.
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] if the test command times out,
/// or [`T3strError::Io`] if the process cannot be spawned.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    // Try npm test first to get stdout (works with mocha, jest, etc.)
    let npm_result = run_npm_test(repo_dir, filter).await;

    // Try npx jest --json for structured output
    let jest_result = run_jest_json(repo_dir, filter).await;

    let (results, combined_output) = merge_results(npm_result, jest_result);

    let summary = build_summary(&results);
    Ok(TestSuite {
        language: Language::Javascript,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined_output, RAW_OUTPUT_MAX)),
    })
}

/// Intermediate result from running a JS test command.
struct JsRunOutput {
    /// Parsed test results.
    results: Vec<TestResult>,
    /// Combined stdout + stderr.
    output: String,
}

/// Run `npm test` and parse stdout with mocha text parser.
async fn run_npm_test(repo_dir: &Path, filter: Option<&str>) -> Result<JsRunOutput, T3strError> {
    let mut args: Vec<String> = vec![String::from("test"), String::from("--")];
    if let Some(f) = filter {
        args.push(String::from("--grep"));
        args.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        "npm",
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Javascript,
    )
    .await?;

    let mut output = stdout.clone();
    output.push('\n');
    output.push_str(&stderr);

    let results = mocha_text::parse(&stdout);
    Ok(JsRunOutput { results, output })
}

/// Run `npx jest --json` and parse structured output.
async fn run_jest_json(repo_dir: &Path, filter: Option<&str>) -> Result<JsRunOutput, T3strError> {
    let mut args: Vec<String> = vec![String::from("jest"), String::from("--json")];
    if let Some(f) = filter {
        args.push(String::from("--testPathPattern"));
        args.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        "npx",
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Javascript,
    )
    .await?;

    let mut output = stdout.clone();
    output.push('\n');
    output.push_str(&stderr);

    let results = jest_json::parse(&stdout).unwrap_or_default();
    Ok(JsRunOutput { results, output })
}

/// Parsed results paired with combined command output.
type MergedOutput = (Vec<TestResult>, String);

/// Merge results from npm test and jest json runs.
///
/// Prefers jest JSON results (structured) when available.
/// Falls back to mocha text results from npm test.
fn merge_results(
    npm: Result<JsRunOutput, T3strError>,
    jest: Result<JsRunOutput, T3strError>,
) -> MergedOutput {
    // Prefer jest JSON if it produced results.
    if let Ok(ref j) = jest {
        if !j.results.is_empty() {
            let output = match npm {
                Ok(n) => {
                    let mut combined = n.output;
                    combined.push('\n');
                    combined.push_str(&j.output);
                    combined
                }
                Err(_) => j.output.clone(),
            };
            return (j.results.clone(), output);
        }
    }

    // Fall back to npm test / mocha results.
    if let Ok(n) = npm {
        let mut output = n.output;
        if let Ok(j) = jest {
            output.push('\n');
            output.push_str(&j.output);
        }
        return (n.results, output);
    }

    // Both failed — return jest output if available for diagnostics.
    if let Ok(j) = jest {
        return (j.results, j.output);
    }

    (Vec::new(), String::new())
}
