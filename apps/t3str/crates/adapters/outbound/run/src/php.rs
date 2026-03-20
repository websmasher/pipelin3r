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

/// Run `PHPUnit` with `JUnit` XML output and parse.
async fn run_phpunit(
    phpunit_bin: &str,
    repo_dir: &Path,
    filter: Option<&str>,
) -> Result<(Vec<t3str_domain_types::TestResult>, String), T3strError> {
    let output_file = repo_dir.join("test-output.xml");
    let output_path = output_file.to_string_lossy().into_owned();

    let log_arg = format!("--log-junit={output_path}");
    let mut args: Vec<String> = vec![log_arg];
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
    let results = if output_file.exists() {
        let xml = tokio::fs::read_to_string(&output_file)
            .await
            .map_err(T3strError::Io)?;
        junit_xml::parse(&xml).unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok((results, output))
}
