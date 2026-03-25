//! Integration tests for `PipelineContext` — input/output verification,
//! `run_local`, remote temp dir behavior, and `AgentStep` construction.
#![allow(
    unused_crate_dependencies,
    reason = "integration test: deps used by lib not by test binary"
)]
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

use std::path::PathBuf;
use std::sync::Arc;

use pipelin3r::{AgentConfig, AgentStep, Executor, PipelineContext, PipelineError};
use shedul3r_rs_sdk::ClientConfig;

// ── Helpers ────────────────────────────────────────────────────────

/// Create a local executor in dry-run mode.
fn local_executor(capture_dir: PathBuf) -> Executor {
    Executor::with_defaults().unwrap().with_dry_run(capture_dir)
}

/// Create a remote executor in dry-run mode (non-localhost URL).
fn remote_executor(capture_dir: PathBuf) -> Executor {
    let config = ClientConfig {
        base_url: String::from("https://remote.example.com"),
        ..ClientConfig::default()
    };
    Executor::new(&config).unwrap().with_dry_run(capture_dir)
}

/// Build a minimal `AgentStep` with the given inputs and outputs.
fn make_step(name: &str, inputs: Vec<&str>, outputs: Vec<&str>) -> AgentStep {
    AgentStep {
        config: AgentConfig::new(name, "test prompt"),
        inputs: inputs.into_iter().map(String::from).collect(),
        outputs: outputs.into_iter().map(String::from).collect(),
    }
}

// ── 1. Input verification ──────────────────────────────────────────

/// `run_agent` with a missing input file returns an error with a clear message.
#[tokio::test]
async fn run_agent_missing_input_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir);

    let step = make_step("missing-input", vec!["nonexistent.txt"], vec![]);
    let result = ctx.run_agent(step).await;

    assert!(result.is_err(), "should fail when input file is missing");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("nonexistent.txt"),
        "error should mention the missing file, got: {err_msg}"
    );
    assert!(
        err_msg.contains("missing-input"),
        "error should mention the step name, got: {err_msg}"
    );
}

/// `run_agent` with an existing input file succeeds in dry-run mode.
#[tokio::test]
async fn run_agent_with_existing_input_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(base_dir.join("data.txt"), b"some data").unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir);

    let step = make_step("has-input", vec!["data.txt"], vec![]);
    let result = ctx.run_agent(step).await;

    assert!(result.is_ok(), "should succeed with existing input file");
    assert!(
        result.unwrap().success,
        "dry-run agent result should be success"
    );
}

/// `run_agent_batch` with a missing input for one item returns error for that item only.
#[tokio::test]
async fn run_agent_batch_missing_input_per_item() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    // Only create the file for item "exists".
    std::fs::write(base_dir.join("exists.txt"), b"here").unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir);

    let items = vec!["exists", "missing"];
    let results = ctx
        .run_agent_batch(items, 2, |item: &&str| {
            let input_file = format!("{item}.txt");
            AgentStep {
                config: AgentConfig::new(format!("batch-{item}"), "test prompt"),
                inputs: vec![input_file],
                outputs: vec![],
            }
        })
        .await;

    assert_eq!(results.len(), 2, "should have results for both items");

    // Find each result by item name.
    let exists_result = results.iter().find(|(item, _)| **item == *"exists");
    let missing_result = results.iter().find(|(item, _)| **item == *"missing");

    assert!(
        exists_result.is_some(),
        "should have result for 'exists' item"
    );
    assert!(
        exists_result.unwrap().1.is_ok(),
        "existing input item should succeed"
    );

    assert!(
        missing_result.is_some(),
        "should have result for 'missing' item"
    );
    assert!(
        missing_result.unwrap().1.is_err(),
        "missing input item should fail"
    );
}

/// Empty items list is valid and returns empty results.
#[tokio::test]
async fn run_agent_batch_empty_items_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir);

    let items: Vec<String> = vec![];
    let results = ctx
        .run_agent_batch(items, 2, |_item: &String| {
            make_step("empty-batch", vec![], vec![])
        })
        .await;

    assert!(
        results.is_empty(),
        "empty items should produce empty results"
    );
}

// ── 5. Output verification ─────────────────────────────────────────

/// `run_agent` in dry-run produces a warning when output not found (dry-run won't
/// produce real outputs). We verify the agent still succeeds (warnings are non-fatal).
#[tokio::test]
async fn run_agent_dry_run_missing_output_still_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir.clone());

    // Declare an output. Dry-run now creates placeholder expected outputs.
    let step = make_step("output-check", vec![], vec!["result.txt"]);
    let result = ctx.run_agent(step).await;

    assert!(
        result.is_ok(),
        "dry-run should succeed and materialize placeholder outputs"
    );
    assert!(
        result.unwrap().success,
        "dry-run result should report success"
    );
    assert!(
        base_dir.join("result.txt").is_file(),
        "dry-run should produce the declared placeholder output file"
    );
}

// ── 6-8. run_local ──────────────────────────────────────────────────

/// `run_local` passes `base_dir` to the closure.
#[test]
fn run_local_passes_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir.clone());

    let mut received_path = PathBuf::new();
    let result = ctx.run_local("local-test", |path| {
        received_path = path.to_path_buf();
        Ok(())
    });

    assert!(result.is_ok(), "run_local should succeed");
    assert_eq!(
        received_path, base_dir,
        "closure should receive the base_dir"
    );
}

/// `run_local` propagates closure errors.
#[test]
fn run_local_propagates_errors() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir);

    let result = ctx.run_local("error-test", |_path| {
        Err(PipelineError::Config(String::from("deliberate failure")))
    });

    assert!(result.is_err(), "run_local should propagate closure error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("deliberate failure"),
        "error should contain closure message, got: {err_msg}"
    );
}

/// `run_local` succeeds when closure returns Ok.
#[test]
fn run_local_succeeds_on_ok() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir.clone());

    // Write a file inside the closure to verify it actually ran.
    let result = ctx.run_local("ok-test", |path| {
        std::fs::write(path.join("output.txt"), b"written by closure")
            .map_err(|e| PipelineError::Transport(format!("write failed: {e}")))?;
        Ok(())
    });

    assert!(result.is_ok(), "run_local should succeed");
    assert!(
        base_dir.join("output.txt").is_file(),
        "closure should have created the output file"
    );
}

// ── 9-11. Remote temp dir ───────────────────────────────────────────

/// For a remote executor, verify the `work_dir` in dry-run capture is a temp dir (not `base_dir`).
#[tokio::test]
async fn remote_executor_uses_temp_dir_not_base_dir() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(base_dir.join("input.txt"), b"remote input data").unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(remote_executor(capture_dir.clone()));
    let ctx = PipelineContext::new(executor, base_dir.clone());

    let step = make_step("remote-step", vec!["input.txt"], vec![]);
    let result = ctx.run_agent(step).await;

    assert!(result.is_ok(), "remote dry-run should succeed");

    // Read the meta.json to check the work_dir used.
    let meta_path = capture_dir.join("remote-step").join("0").join("meta.json");
    let meta_content = std::fs::read_to_string(&meta_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_content).unwrap();

    let work_dir_str = meta
        .get("workDir")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    // The work_dir should be a temp directory, NOT the base_dir.
    assert!(
        !work_dir_str.is_empty(),
        "workDir should be present in meta.json"
    );
    assert!(
        !work_dir_str.contains(base_dir.to_str().unwrap_or("")),
        "remote work_dir should NOT be the base_dir; got: {work_dir_str}"
    );
}

/// For a remote executor, verify inputs are copied to the temp dir.
#[tokio::test]
async fn remote_executor_copies_inputs_to_temp_dir() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(base_dir.join("source.txt"), b"source content").unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(remote_executor(capture_dir.clone()));
    let ctx = PipelineContext::new(executor, base_dir);

    let step = make_step("remote-copy", vec!["source.txt"], vec![]);
    let result = ctx.run_agent(step).await;

    assert!(result.is_ok(), "remote dry-run with input should succeed");

    // The meta.json should list the input file in the temp work dir.
    let meta_path = capture_dir.join("remote-copy").join("0").join("meta.json");
    let meta_content = std::fs::read_to_string(&meta_path).unwrap();

    // workDirFiles should contain our copied input.
    assert!(
        meta_content.contains("source.txt"),
        "temp dir should contain the copied input file; meta: {meta_content}"
    );
}

/// Remote output copy-back is hard to test without real execution.
/// This test documents the limitation: in dry-run mode the agent doesn't
/// produce real outputs, so we can only verify the step completes without error.
#[tokio::test]
async fn remote_executor_output_copy_back_limitation() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(base_dir.join("in.txt"), b"data").unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(remote_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir.clone());

    // Declare an output. Dry-run now creates placeholder expected outputs.
    let step = make_step("remote-output", vec!["in.txt"], vec!["out.txt"]);
    let result = ctx.run_agent(step).await;

    assert!(
        result.is_ok(),
        "remote dry-run should succeed and materialize placeholder outputs"
    );
    assert!(
        base_dir.join("out.txt").is_file(),
        "dry-run should produce the declared placeholder output file"
    );
}

// ── 12-13. AgentStep construction ───────────────────────────────────

/// `AgentStep` with empty inputs and outputs is valid.
#[tokio::test]
async fn agent_step_empty_inputs_and_outputs_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir));
    let ctx = PipelineContext::new(executor, base_dir);

    let step = make_step("empty-step", vec![], vec![]);
    let result = ctx.run_agent(step).await;

    assert!(result.is_ok(), "step with no inputs/outputs should succeed");
    assert!(
        result.unwrap().success,
        "dry-run result should report success"
    );
}

/// `AgentStep` config gets `work_dir` and `expect_outputs` set by context.
#[tokio::test]
async fn agent_step_config_gets_work_dir_and_outputs_set() {
    let dir = tempfile::tempdir().unwrap();
    let base_dir = dir.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();

    let capture_dir = dir.path().join("capture");
    let executor = Arc::new(local_executor(capture_dir.clone()));
    let ctx = PipelineContext::new(executor, base_dir.clone());

    let step = make_step("config-test", vec![], vec!["output.json"]);

    // The step's config should NOT have work_dir or expect_outputs before run_agent.
    assert!(
        step.config.work_dir.is_none(),
        "work_dir should be None before context runs it"
    );
    assert!(
        step.config.expect_outputs.is_empty(),
        "expect_outputs should be empty before context runs it"
    );

    let result = ctx.run_agent(step).await;
    assert!(result.is_ok(), "should succeed in dry-run");

    // Verify the capture shows the correct work_dir (should be base_dir for local).
    let meta_path = capture_dir.join("config-test").join("0").join("meta.json");
    let meta_content = std::fs::read_to_string(&meta_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_content).unwrap();

    let work_dir_str = meta
        .get("workDir")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    assert!(
        work_dir_str.contains(base_dir.to_str().unwrap_or("")),
        "local executor should set work_dir to base_dir; got: {work_dir_str}"
    );
}
