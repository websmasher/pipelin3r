//! Rust test execution via `cargo test`.
//!
//! Runs `cargo test` and parses the human-readable text output from stdout.
//! Sets `CARGO_HOME` and `CARGO_TARGET_DIR` to isolated directories within
//! the repo to avoid polluting the user's global Cargo state.

use std::path::{Path, PathBuf};

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::cargo_text;

/// Timeout for Rust test execution (10 minutes — includes compilation).
const TIMEOUT_SECS: u64 = 600;

/// Maximum characters to keep in raw output.
const RAW_OUTPUT_MAX: usize = 2000;

/// A `.cargo/config` file that was temporarily renamed before running tests.
struct HiddenConfig {
    original: PathBuf,
    backup: PathBuf,
}

/// Temporarily rename `.cargo/config.toml` and `.cargo/config` so that
/// custom test-runner configurations (e.g. `runner = "sudo -E rlwrap"`)
/// do not interfere with `cargo test`.  Returns the list of files that
/// were successfully renamed so they can be restored afterwards.
async fn hide_cargo_configs(repo_dir: &Path) -> Vec<HiddenConfig> {
    let candidates = [
        repo_dir.join(".cargo/config.toml"),
        repo_dir.join(".cargo/config"),
    ];

    let mut hidden = Vec::new();

    for original in candidates {
        let mut backup = original.clone();
        let name = original
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default(); // allow: infallible — paths are hardcoded ASCII above
        let backup_name = format!("{name}.bak");
        backup.set_file_name(&backup_name);

        if tokio::fs::rename(&original, &backup).await.is_ok() {
            hidden.push(HiddenConfig { original, backup });
        }
    }

    hidden
}

/// Restore previously hidden `.cargo/config*` files.
async fn restore_cargo_configs(hidden: Vec<HiddenConfig>) {
    for entry in hidden {
        let _ignored = tokio::fs::rename(&entry.backup, &entry.original).await;
    }
}

/// Execute Rust tests in the given directory.
///
/// Runs `cargo test` with isolated `CARGO_HOME` and `CARGO_TARGET_DIR`.
/// Parses the text output from stdout using [`cargo_text::parse`].
/// If a filter is provided, it is passed after `--` to the test binary.
///
/// Before running, any `.cargo/config.toml` or `.cargo/config` in the repo
/// is temporarily renamed so that custom test-runner settings do not
/// interfere with output capture.
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

    // Hide custom cargo configs that may specify incompatible test runners.
    let hidden = hide_cargo_configs(repo_dir).await;

    let result = run_command(
        "cargo",
        &args,
        repo_dir,
        &env_vars,
        TIMEOUT_SECS,
        Language::Rust,
    )
    .await;

    // Always restore configs, even if `run_command` failed.
    restore_cargo_configs(hidden).await;

    let (stdout, stderr, _exit_code) = result?;

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
