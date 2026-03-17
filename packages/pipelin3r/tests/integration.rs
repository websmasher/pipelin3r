//! Integration tests for pipelin3r: agent dry-run and transform flows.

#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct fs access for test fixtures"
)]

// Suppress unused-crate-dependencies for test binary (these are used by the library).
use serde_json as _;
use shedul3r_rs_sdk as _;
use tempfile as _;
use thiserror as _;
use toml as _;
use tracing as _;

use std::path::Path;

use pipelin3r::{
    AgentConfig, Auth, Executor, Model, TemplateFiller, TransformBuilder, TransformResult,
};

/// Verify that a single agent dry-run creates the expected capture files.
#[tokio::test]
#[allow(clippy::unwrap_used)] // reason: integration test assertions
async fn agent_dry_run_creates_capture_files() {
    let dir = tempfile::tempdir().unwrap();
    let capture_dir = dir.path().to_path_buf();

    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(capture_dir.clone());

    // Build a prompt from a template.
    let template_filler = TemplateFiller::new().set("{{PACKAGE}}", "my-parser");
    let prompt_text = template_filler.fill("Implement tests for {{PACKAGE}}");

    // Create a work directory with an input file.
    let work_dir = dir.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("input.txt"), b"test input").unwrap();

    let config = AgentConfig {
        work_dir: Some(work_dir),
        ..AgentConfig::new("test-step", &prompt_text)
    };

    let result = executor.run_agent(&config).await.unwrap();

    assert!(result.success, "dry-run agent should succeed");

    // The capture directory is base_dir / step-slug / counter.
    // Step name "test-step" slugifies to "test-step", counter starts at 0.
    let step_dir = capture_dir.join("test-step").join("0");

    assert_file_exists(&step_dir.join("prompt.md"), "prompt.md in capture dir");
    assert_file_exists(&step_dir.join("task.yaml"), "task.yaml in capture dir");
    assert_file_exists(&step_dir.join("meta.json"), "meta.json in capture dir");

    // Verify prompt content matches the filled template.
    let prompt_content = std::fs::read_to_string(step_dir.join("prompt.md")).unwrap();
    assert_eq!(
        prompt_content, "Implement tests for my-parser",
        "prompt.md should contain the filled template"
    );

    // Verify meta.json contains work directory path and files.
    let meta_content = std::fs::read_to_string(step_dir.join("meta.json")).unwrap();
    assert!(
        meta_content.contains("workDir"),
        "meta.json should contain workDir key"
    );
    assert!(
        meta_content.contains("input.txt"),
        "meta.json should list work dir files"
    );
}

/// Verify that a transform reads, transforms, and writes files end-to-end.
#[test]
#[allow(clippy::unwrap_used)] // reason: integration test assertions
fn transform_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    // Create input files.
    let input_dir = base.join("input");
    std::fs::create_dir_all(&input_dir).unwrap();
    std::fs::write(input_dir.join("a.txt"), b"hello").unwrap();
    std::fs::write(input_dir.join("b.txt"), b"world").unwrap();

    let output_dir = base.join("output");

    let result: TransformResult = TransformBuilder::new("e2e-transform")
        .input_file(&input_dir.join("a.txt"))
        .input_file(&input_dir.join("b.txt"))
        .apply({
            let out = output_dir.clone();
            move |inputs| {
                // Concatenate all inputs into a single output file.
                let mut combined = Vec::new();
                for (_, content) in &inputs {
                    if !combined.is_empty() {
                        combined.push(b'\n');
                    }
                    combined.extend_from_slice(content);
                }
                Ok(vec![(out.join("combined.txt"), combined)])
            }
        })
        .execute()
        .unwrap();

    assert_eq!(result.files_read, 2, "should read 2 input files");
    assert_eq!(result.files_written, 1, "should write 1 merged output file");

    let combined = std::fs::read_to_string(output_dir.join("combined.txt")).unwrap();
    assert_eq!(
        combined, "hello\nworld",
        "combined output should contain both inputs"
    );
}

// ── Regression tests ────────────────────────────────────────────

/// Verify that dry-run capture includes auth environment keys in meta.json.
#[tokio::test]
#[allow(clippy::unwrap_used)] // reason: integration test assertions
async fn regression_dry_run_captures_auth_in_meta() {
    let dir = tempfile::tempdir().unwrap();
    let capture_dir = dir.path().to_path_buf();

    let auth = Auth::ApiKey(String::from("sk-test-key"));
    let executor = Executor::with_defaults()
        .unwrap()
        .with_default_auth(auth)
        .with_dry_run(capture_dir.clone());

    let config = AgentConfig::new("auth-capture-test", "test prompt");
    let _result = executor.run_agent(&config).await.unwrap();

    let meta_path = capture_dir
        .join("auth-capture-test")
        .join("0")
        .join("meta.json");
    let meta_content = std::fs::read_to_string(&meta_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_content).unwrap();

    assert!(
        meta.get("environment").is_some(),
        "meta.json must contain 'environment' key, got: {meta_content}"
    );
    let env_arr = meta
        .get("environment")
        .and_then(serde_json::Value::as_array);
    assert!(env_arr.is_some(), "environment must be an array");
    assert!(
        !env_arr.unwrap().is_empty(),
        "environment array must not be empty when auth is provided"
    );
}

/// Verify that dry-run capture includes work-dir file paths in meta.json.
#[tokio::test]
#[allow(clippy::unwrap_used)] // reason: integration test assertions
async fn regression_dry_run_captures_work_dir_files_in_meta() {
    let dir = tempfile::tempdir().unwrap();
    let capture_dir = dir.path().join("capture");
    let work_dir = dir.path().join("work");

    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("input.txt"), b"hello world").unwrap();
    std::fs::write(work_dir.join("config.json"), b"{}").unwrap();

    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(capture_dir.clone());

    let config = AgentConfig {
        work_dir: Some(work_dir),
        ..AgentConfig::new("workdir-capture-test", "test prompt")
    };
    let _result = executor.run_agent(&config).await.unwrap();

    let meta_path = capture_dir
        .join("workdir-capture-test")
        .join("0")
        .join("meta.json");
    let meta_content = std::fs::read_to_string(&meta_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_content).unwrap();

    assert!(
        meta.get("workDirFiles").is_some(),
        "meta.json must contain 'workDirFiles' key, got: {meta_content}"
    );
    let files_arr = meta
        .get("workDirFiles")
        .and_then(serde_json::Value::as_array);
    assert!(files_arr.is_some(), "workDirFiles must be an array");
    let files = files_arr.unwrap();
    assert_eq!(files.len(), 2, "workDirFiles must list both work dir files");
    let file_names: Vec<&str> = files.iter().filter_map(serde_json::Value::as_str).collect();
    assert!(
        file_names.contains(&"input.txt"),
        "workDirFiles must contain input.txt"
    );
    assert!(
        file_names.contains(&"config.json"),
        "workDirFiles must contain config.json"
    );
}

/// Verify dry-run with model config produces correct YAML.
#[tokio::test]
async fn agent_dry_run_with_model_in_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let capture_dir = dir.path().to_path_buf();

    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(capture_dir.clone());

    let config = AgentConfig {
        model: Some(Model::Opus4_6),
        ..AgentConfig::new("model-test", "test prompt")
    };

    let result = executor.run_agent(&config).await.unwrap();
    assert!(result.success, "dry-run should succeed");

    let task_yaml =
        std::fs::read_to_string(capture_dir.join("model-test").join("0").join("task.yaml"))
            .unwrap();
    assert!(
        task_yaml.contains("claude-opus-4-6"),
        "task YAML must contain the resolved model ID, got: {task_yaml}"
    );
}

/// Assert that a path exists and is a file.
fn assert_file_exists(path: &Path, context: &str) {
    assert!(
        path.is_file(),
        "{context}: {} should be a file",
        path.display()
    );
}
