//! Go test execution via `go test -json`.
//!
//! Runs `go test -json ./...` and parses the NDJSON output from stdout.
//! Sets `GOPATH` to an isolated directory within the repo.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestSuite};

use crate::helpers::{build_summary, run_command, truncate_output};
use crate::parsers::go_json;

/// Detect the Go module path from source files in the given directory.
///
/// Reads the first `.go` file found at the top level of `repo_dir`, extracts
/// the `package <name>` declaration, and returns it as the module path. Falls
/// back to the directory name if no `.go` file or package declaration is found.
async fn detect_module_path(repo_dir: &Path) -> String {
    let fallback = repo_dir
        .file_name()
        .map_or_else(|| String::from("module"), |n| n.to_string_lossy().into_owned());

    let Ok(mut entries) = tokio::fs::read_dir(repo_dir).await else {
        return fallback;
    };

    loop {
        let Ok(maybe_entry) = entries.next_entry().await else {
            break;
        };
        let Some(entry) = maybe_entry else {
            break;
        };
        let path = entry.path();
        let is_go = path.extension().is_some_and(|ext| ext == "go");
        if !is_go || !path.is_file() {
            continue;
        }

        let Ok(contents) = tokio::fs::read_to_string(&path).await else {
            continue;
        };

        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("package ") {
                if let Some(name) = trimmed.split_whitespace().nth(1) {
                    if !name.is_empty() {
                        return name.to_owned();
                    }
                }
            }
        }
    }

    fallback
}

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
    let env_vars = [("GOPATH", gopath_str.as_str())];

    // If no go.mod exists, initialise a module so `go test` works.
    // Use the actual package name from source files so import paths resolve.
    let mut created_gomod = false;
    if !repo_dir.join("go.mod").exists() {
        created_gomod = true;
        let module_path = detect_module_path(repo_dir).await;
        let _ = run_command(
            "go",
            &["mod", "init", &module_path],
            repo_dir,
            &env_vars,
            TIMEOUT_SECS,
            Language::Go,
        )
        .await?;

        let _ = run_command(
            "go",
            &["mod", "tidy"],
            repo_dir,
            &env_vars,
            TIMEOUT_SECS,
            Language::Go,
        )
        .await?;
    }

    let mut args: Vec<&str> = vec!["test", "-json"];

    // Disable `go vet` for repos without go.mod — Go 1.24 runs vet by
    // default and rejects old patterns like non-constant format strings.
    if created_gomod {
        args.insert(2, "-vet=off");
    }

    if let Some(f) = filter {
        args.push("-run");
        args.push(f);
    }

    args.push("./...");

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
