//! Python test execution via `pytest`.
//!
//! Runs `pytest` with `--junitxml` for structured output, then parses the
//! resulting XML file. Prefers a virtualenv Python (``.venv/bin/python3``)
//! when available, falling back to the system ``python3``.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::junit_xml;

/// Timeout for pytest execution.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute Python tests in the given directory.
///
/// Runs `pytest --junitxml=<path> -v` and parses the `JUnit` XML output.
/// If a virtualenv exists at `.venv/bin/python3`, it is used instead of
/// the system Python. When pytest reports exit code 5 (no tests collected),
/// a second attempt discovers test files explicitly via `find`.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let venv_python = repo_dir.join(".venv/bin/python3");
    let py = if venv_python.exists() {
        venv_python.to_string_lossy().into_owned()
    } else {
        String::from("python3")
    };

    let output_file = repo_dir.join("test-output.xml");
    let output_path = output_file.to_string_lossy().into_owned();
    let junitxml_flag = format!("--junitxml={output_path}");

    let mut args: Vec<&str> = vec!["-m", "pytest", &junitxml_flag, "-v"];

    if let Some(f) = filter {
        args.push("-k");
        args.push(f);
    }

    let (stdout, stderr, exit_code) =
        run_command(&py, &args, repo_dir, &[], TIMEOUT_SECS, Language::Python).await?;

    let mut combined = stdout.clone();
    combined.push('\n');
    combined.push_str(&stderr);

    // Exit code 5 means no tests were collected by default discovery.
    // Try explicit file discovery as a fallback.
    if exit_code == 5 {
        let retry_result = retry_with_explicit_files(&py, &junitxml_flag, repo_dir, filter).await;

        if let Ok((extra_stdout, extra_stderr)) = retry_result {
            combined.push('\n');
            combined.push_str(&extra_stdout);
            combined.push('\n');
            combined.push_str(&extra_stderr);
        }
    }

    // Parse JUnit XML output file
    let results = if output_file.exists() {
        let xml_content = tokio::fs::read_to_string(&output_file)
            .await
            .map_err(T3strError::Io)?;

        // Clean up the output file
        let _ignore = tokio::fs::remove_file(&output_file).await;

        junit_xml::parse(&xml_content).unwrap_or_default()
    } else {
        Vec::new()
    };

    let summary = build_summary(&results);

    Ok(TestSuite {
        language: Language::Python,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}

/// Retry pytest with explicitly discovered test files.
///
/// Returns `(stdout, stderr)` from the retry attempt.
async fn retry_with_explicit_files(
    python: &str,
    junitxml_flag: &str,
    repo_dir: &Path,
    filter: Option<&str>,
) -> Result<(String, String), T3strError> {
    // Find test files in common directories
    let (found_stdout, _, _) = run_command(
        "find",
        &[".", "-maxdepth", "4", "-name", "test_*.py", "-type", "f"],
        repo_dir,
        &[],
        30,
        Language::Python,
    )
    .await?;

    let files: Vec<&str> = found_stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();

    if files.is_empty() {
        return Ok((String::new(), String::new()));
    }

    let mut retry_args: Vec<&str> = vec!["-m", "pytest", junitxml_flag, "-v"];
    for f in &files {
        retry_args.push(f);
    }
    if let Some(flt) = filter {
        retry_args.push("-k");
        retry_args.push(flt);
    }

    let (stdout, stderr, _) = run_command(
        python,
        &retry_args,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Python,
    )
    .await?;

    Ok((stdout, stderr))
}
