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

/// A `.cargo/config` file whose `runner` setting was stripped.
struct SanitizedConfig {
    path: PathBuf,
    original_content: String,
}

/// Strip `runner = ...` lines from `.cargo/config.toml` and `.cargo/config`
/// so custom test runners (e.g. `runner = "sudo -E rlwrap"`) don't interfere
/// with `cargo test`. Preserves all other settings (rustflags, etc.).
/// Returns the original content so it can be restored afterwards.
async fn sanitize_cargo_configs(repo_dir: &Path) -> Vec<SanitizedConfig> {
    let candidates = [
        repo_dir.join(".cargo/config.toml"),
        repo_dir.join(".cargo/config"),
    ];

    let mut sanitized = Vec::new();

    for path in candidates {
        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            continue;
        };

        // Only modify if there's actually a runner setting.
        if !content.lines().any(|l| l.trim().starts_with("runner")) {
            continue;
        }

        let cleaned: String = content
            .lines()
            .filter(|line| !line.trim().starts_with("runner"))
            .collect::<Vec<_>>()
            .join("\n");

        if tokio::fs::write(&path, &cleaned).await.is_ok() {
            sanitized.push(SanitizedConfig {
                path,
                original_content: content,
            });
        }
    }

    sanitized
}

/// Restore original `.cargo/config*` content after test execution.
async fn restore_cargo_configs(configs: Vec<SanitizedConfig>) {
    for entry in configs {
        let _ignored = tokio::fs::write(&entry.path, &entry.original_content).await;
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

    // Strip custom test-runner settings from cargo configs.
    let sanitized = sanitize_cargo_configs(repo_dir).await;

    let result = run_command(
        "cargo",
        &args,
        repo_dir,
        &env_vars,
        TIMEOUT_SECS,
        Language::Rust,
    )
    .await;

    // Always restore original configs, even if `run_command` failed.
    restore_cargo_configs(sanitized).await;

    let (stdout, stderr, exit_code) = result?;

    let mut combined = stdout.clone();
    combined.push('\n');
    combined.push_str(&stderr);

    // Parse cargo test text output from stdout
    let mut results = cargo_text::parse(&stdout);

    // If compilation failed (no test results and non-zero exit), retry with
    // individual lib crates. Workspace builds often fail because a binary crate
    // has extra dependencies that don't compile, while the library tests are fine.
    if results.is_empty() && exit_code != 0 {
        if let Some(lib_results) =
            retry_lib_crates(repo_dir, &env_vars, filter, &mut combined).await
        {
            results = lib_results;
        }
    }

    let summary = build_summary(&results);

    Ok(TestSuite {
        language: Language::Rust,
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        results,
        summary,
        raw_output: Some(truncate_output(&combined, RAW_OUTPUT_MAX)),
    })
}

/// When `cargo test` fails for the whole workspace, try each lib crate
/// individually. Many repos have bin crates with extra dependencies that
/// fail to compile while the library (where the tests live) is fine.
async fn retry_lib_crates(
    repo_dir: &Path,
    env_vars: &[crate::helpers::EnvVar<'_>],
    filter: Option<&str>,
    combined: &mut String,
) -> Option<Vec<t3str_domain_types::TestResult>> {
    // Find Cargo.toml files in subdirectories that define lib crates.
    let Ok(mut entries) = tokio::fs::read_dir(repo_dir).await else {
        return None;
    };

    let mut lib_crates = Vec::new();

    loop {
        let Ok(maybe) = entries.next_entry().await else {
            break;
        };
        let Some(entry) = maybe else {
            break;
        };
        let path = entry.path();
        if path.is_dir() {
            let cargo_toml = path.join("Cargo.toml");
            if cargo_toml.is_file() {
                if let Ok(content) = tokio::fs::read_to_string(&cargo_toml).await {
                    // Look for [lib] section — indicates this is a library crate.
                    if content.contains("[lib]") || content.contains("lib.rs") {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            lib_crates.push(name.to_owned());
                        }
                    }
                }
            }
        }
    }

    // Also check root Cargo.toml for a single-crate repo with [lib].
    if lib_crates.is_empty() {
        let root_toml = repo_dir.join("Cargo.toml");
        if let Ok(content) = tokio::fs::read_to_string(&root_toml).await {
            if content.contains("[lib]") {
                // Single crate — try `cargo test --lib`
                let mut args: Vec<&str> = vec!["test", "--lib"];
                if let Some(f) = filter {
                    args.push("--");
                    args.push(f);
                }
                if let Ok((stdout, stderr, _)) =
                    run_command("cargo", &args, repo_dir, env_vars, TIMEOUT_SECS, Language::Rust)
                        .await
                {
                    combined.push('\n');
                    combined.push_str(&stdout);
                    combined.push('\n');
                    combined.push_str(&stderr);
                    let results = cargo_text::parse(&stdout);
                    if !results.is_empty() {
                        return Some(results);
                    }
                }
            }
        }
        return None;
    }

    // Try each lib crate with `-p <name>`.
    let mut all_results = Vec::new();

    for crate_name in &lib_crates {
        let mut args: Vec<&str> = vec!["test", "-p", crate_name.as_str()];
        if let Some(f) = filter {
            args.push("--");
            args.push(f);
        }
        if let Ok((stdout, stderr, _)) =
            run_command("cargo", &args, repo_dir, env_vars, TIMEOUT_SECS, Language::Rust).await
        {
            combined.push('\n');
            combined.push_str(&stdout);
            combined.push('\n');
            combined.push_str(&stderr);
            all_results.extend(cargo_text::parse(&stdout));
        }
    }

    if all_results.is_empty() {
        None
    } else {
        Some(all_results)
    }
}
