//! Go test execution via `go test -json`.
//!
//! Runs `go test -json ./...` and parses the NDJSON output from stdout.
//! Sets `GOPATH` to an isolated directory within the repo.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::go_json;

/// Timeout for Go test execution.
const TIMEOUT_SECS: u64 = 300;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute Go tests in the given directory.
///
/// Runs `go test -json ./...` with an isolated `GOPATH`. If a filter is
/// provided, it is passed via `-run <filter>`. The JSON output from stdout
/// is parsed using [`go_json::parse`].
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let gopath = repo_dir.join(".gopath");
    let gopath_str = gopath.to_string_lossy().into_owned();

    let mut args: Vec<&str> = vec!["test", "-json"];

    if let Some(f) = filter {
        args.push("-run");
        args.push(f);
    }

    args.push("./...");

    let env_vars = [("GOPATH", gopath_str.as_str())];

    let (stdout, stderr, _exit_code) =
        run_command("go", &args, repo_dir, &env_vars, TIMEOUT_SECS, Language::Go).await?;

    let mut combined = stdout.clone();
    combined.push('\n');
    combined.push_str(&stderr);

    // Parse go test JSON output from stdout
    let results = go_json::parse(&stdout).unwrap_or_default();

    let summary = build_summary(&results);

    Ok(TestSuite {
        language: Language::Go,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}
