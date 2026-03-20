//! Java test execution and output parsing.
//!
//! Detects Maven (`pom.xml`) or Gradle (`build.gradle` / `build.gradle.kts`)
//! and runs the appropriate test command, then collects `JUnit` XML reports.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestResult, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::junit_xml;

/// Timeout for Java test execution in seconds.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute Java tests in the given directory.
///
/// Detects the build system by checking for `pom.xml` (Maven) or
/// `build.gradle` / `build.gradle.kts` (Gradle). Runs the appropriate
/// test command, then collects and parses `JUnit` XML report files.
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] if the test command times out,
/// [`T3strError::NoTestFramework`] if no build file is found,
/// or [`T3strError::Io`] if the process cannot be spawned.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let pom = repo_dir.join("pom.xml");
    let gradle_groovy = repo_dir.join("build.gradle");
    let gradle_kotlin = repo_dir.join("build.gradle.kts");

    let (results, combined) = if pom.exists() {
        run_maven(repo_dir, filter).await?
    } else if gradle_groovy.exists() || gradle_kotlin.exists() {
        run_gradle(repo_dir, filter).await?
    } else {
        return Err(T3strError::NoTestFramework {
            language: Language::Java,
            repo_dir: repo_dir.to_string_lossy().into_owned(),
        });
    };

    let summary = build_summary(&results);
    Ok(TestSuite {
        language: Language::Java,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}

/// Run Maven tests and collect surefire XML reports.
async fn run_maven(
    repo_dir: &Path,
    filter: Option<&str>,
) -> Result<(Vec<TestResult>, String), T3strError> {
    let mut args_owned: Vec<String> = vec![String::from("test"), String::from("-B")];
    if let Some(f) = filter {
        let mut dtest = String::from("-Dtest=");
        dtest.push_str(f);
        args_owned.push(dtest);
    }

    let arg_refs: Vec<&str> = args_owned.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        "mvn",
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Java,
    )
    .await?;

    let mut combined = stdout;
    combined.push('\n');
    combined.push_str(&stderr);

    let report_dir = repo_dir.join("target/surefire-reports");
    let results = collect_junit_reports(&report_dir).await;

    Ok((results, combined))
}

/// Run Gradle tests and collect test result XML reports.
async fn run_gradle(
    repo_dir: &Path,
    filter: Option<&str>,
) -> Result<(Vec<TestResult>, String), T3strError> {
    let mut args_owned: Vec<String> = vec![String::from("test")];
    if let Some(f) = filter {
        args_owned.push(String::from("--tests"));
        args_owned.push(String::from(f));
    }

    let arg_refs: Vec<&str> = args_owned.iter().map(String::as_str).collect();
    let (stdout, stderr, _code) = run_command(
        "gradle",
        &arg_refs,
        repo_dir,
        &[],
        TIMEOUT_SECS,
        Language::Java,
    )
    .await?;

    let mut combined = stdout;
    combined.push('\n');
    combined.push_str(&stderr);

    let report_dir = repo_dir.join("build/test-results/test");
    let results = collect_junit_reports(&report_dir).await;

    Ok((results, combined))
}

/// Read all `.xml` files in a directory and parse them as `JUnit` XML.
async fn collect_junit_reports(report_dir: &Path) -> Vec<TestResult> {
    let mut all_results: Vec<TestResult> = Vec::new();

    let Ok(mut entries) = tokio::fs::read_dir(report_dir).await else {
        return all_results;
    };

    loop {
        let Ok(Some(entry)) = entries.next_entry().await else {
            break;
        };

        let path = entry.path();
        let is_xml = path.extension().and_then(std::ffi::OsStr::to_str) == Some("xml");

        if !is_xml {
            continue;
        }

        if let Ok(xml) = tokio::fs::read_to_string(&path).await {
            if let Ok(mut parsed) = junit_xml::parse(&xml) {
                all_results.append(&mut parsed);
            }
        }
    }

    all_results
}
