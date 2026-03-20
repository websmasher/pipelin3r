//! Rust test execution via `cargo test`.
//!
//! Runs `cargo test` and parses the human-readable text output from stdout.
//! Sets `CARGO_HOME` and `CARGO_TARGET_DIR` to isolated directories within
//! the repo to avoid polluting the user's global Cargo state.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::cargo_text;

/// Timeout for Rust test execution (10 minutes — includes compilation).
const TIMEOUT_SECS: u64 = 600;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// Execute Rust tests in the given directory.
///
/// Runs `cargo test` with isolated `CARGO_HOME` and `CARGO_TARGET_DIR`.
/// Parses the text output from stdout using [`cargo_text::parse`].
/// If a filter is provided, it is passed after `--` to the test binary.
pub async fn execute(repo_dir: &Path, filter: Option<&str>) -> Result<TestSuite, T3strError> {
    let cargo_home = repo_dir.join(".cargo-home");
    let cargo_home_str = cargo_home.to_string_lossy().into_owned();
    let target_dir = repo_dir.join("target-t3str");
    let target_dir_str = target_dir.to_string_lossy().into_owned();

    let mut args: Vec<&str> = vec!["test"];

    if let Some(f) = filter {
        args.push("--");
        args.push(f);
    }

    let env_vars = [
        ("CARGO_HOME", cargo_home_str.as_str()),
        ("CARGO_TARGET_DIR", target_dir_str.as_str()),
    ];

    let (stdout, stderr, _exit_code) = run_command(
        "cargo",
        &args,
        repo_dir,
        &env_vars,
        TIMEOUT_SECS,
        Language::Rust,
    )
    .await?;

    let mut combined = stdout.clone();
    combined.push('\n');
    combined.push_str(&stderr);

    // Parse cargo test text output from stdout
    let results = cargo_text::parse(&stdout);

    let summary = build_summary(&results);

    Ok(TestSuite {
        language: Language::Rust,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}
