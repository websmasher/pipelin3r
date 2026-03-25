#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::type_complexity,
    reason = "test code: explicit types for clarity"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct fs access for test fixtures"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use super::*;
use crate::executor::Executor;
use crate::model::Model;

#[test]
fn agent_result_require_success_ok() {
    let result = AgentResult {
        success: true,
        output: String::from("done"),
        output_files: BTreeMap::new(),
    };
    assert!(
        result.require_success().is_ok(),
        "should return Ok for successful agent"
    );
}

#[test]
fn agent_result_require_success_err() {
    let result = AgentResult {
        success: false,
        output: String::from("timeout exceeded"),
        output_files: BTreeMap::new(),
    };
    let err = result.require_success();
    assert!(err.is_err(), "should return Err for failed agent");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("timeout exceeded"),
        "error should contain output: {msg}"
    );
}

#[test]
fn regression_require_success_returns_agent_failed_not_other() {
    let result = AgentResult {
        success: false,
        output: String::from("model timeout"),
        output_files: BTreeMap::new(),
    };
    let err = result.require_success();
    assert!(err.is_err(), "failed agent must return Err");
    assert!(
        matches!(&err, Err(crate::error::PipelineError::AgentFailed { message }) if message == "model timeout"),
        "must be PipelineError::AgentFailed with preserved message, got: {err:?}"
    );
}

#[test]
fn agent_config_new_sets_required_fields() {
    let config = AgentConfig::new("test-step", "do something");
    assert_eq!(config.name, "test-step", "name should be set");
    assert_eq!(config.prompt, "do something", "prompt should be set");
    assert!(config.model.is_none(), "model should default to None");
    assert!(config.work_dir.is_none(), "work_dir should default to None");
    assert!(
        config.execution_timeout.is_none(),
        "execution_timeout should default to None"
    );
    assert!(config.tools.is_none(), "tools should default to None");
    assert!(config.auth.is_none(), "auth should default to None");
    assert!(config.env.is_none(), "env should default to None");
    assert!(
        config.provider_id.is_none(),
        "provider_id should default to None"
    );
    assert!(
        config.max_concurrent.is_none(),
        "max_concurrent should default to None"
    );
    assert!(config.max_wait.is_none(), "max_wait should default to None");
    assert!(config.retry.is_none(), "retry should default to None");
    assert!(
        config.expect_outputs.is_empty(),
        "expect_outputs should default to empty"
    );
    assert!(
        config.request_timeout.is_none(),
        "request_timeout should default to None"
    );
}

#[test]
fn agent_config_struct_update_syntax() {
    let defaults = AgentConfig {
        model: Some(Model::Sonnet4_6),
        execution_timeout: Some(Duration::from_secs(600)),
        ..AgentConfig::new("", "")
    };

    let config = AgentConfig {
        name: String::from("write-article"),
        prompt: String::from("Write an article"),
        work_dir: Some(PathBuf::from("/tmp/work")),
        ..defaults
    };

    assert_eq!(config.name, "write-article", "name should be overridden");
    assert_eq!(
        config.prompt, "Write an article",
        "prompt should be overridden"
    );
    assert!(
        config.model.is_some(),
        "model should be inherited from defaults"
    );
    assert!(
        config.execution_timeout.is_some(),
        "execution_timeout should be inherited from defaults"
    );
    assert!(
        config.work_dir.is_some(),
        "work_dir should be set in override"
    );
}

#[tokio::test]
async fn dry_run_with_agent_config() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-config-test"));

    let config = AgentConfig {
        model: Some(Model::Opus4_6),
        ..AgentConfig::new("test-config", "test prompt")
    };

    let result = executor.run_agent(&config).await.unwrap();
    assert!(result.success, "dry-run should succeed");
    assert!(
        result.output_files.is_empty(),
        "dry-run without expected outputs should have no output files"
    );

    // Read the captured task YAML and verify it contains the opus model ID.
    let task_yaml =
        std::fs::read_to_string("/tmp/pipelin3r-config-test/test-config/0/task.yaml").unwrap();
    assert!(
        task_yaml.contains("claude-opus-4-6"),
        "task YAML must contain the resolved model ID 'claude-opus-4-6', got: {task_yaml}"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-config-test");
}

#[tokio::test]
async fn dry_run_creates_placeholder_expected_outputs() {
    let capture_dir = PathBuf::from("/tmp/pipelin3r-dry-run-placeholders");
    let work_dir = PathBuf::from("/tmp/pipelin3r-dry-run-workdir");
    std::fs::create_dir_all(&work_dir).unwrap();

    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(capture_dir.clone());

    let config = AgentConfig {
        work_dir: Some(work_dir.clone()),
        expect_outputs: vec![String::from("out/result.md")],
        ..AgentConfig::new("placeholder-test", "test prompt")
    };

    let result = executor.run_agent(&config).await.unwrap();
    assert!(result.success, "dry-run should succeed");
    assert!(
        work_dir.join("out/result.md").is_file(),
        "dry-run should create placeholder expected outputs"
    );
    assert!(
        result.output_files.contains_key("out/result.md"),
        "dry-run result should expose placeholder output files"
    );

    let _ = std::fs::remove_dir_all(capture_dir);
    let _ = std::fs::remove_dir_all(work_dir);
}

#[tokio::test]
async fn dry_run_without_prompt_still_captures() {
    // AgentConfig always has a prompt (it's a required field in the constructor).
    // Even an empty prompt should produce a capture.
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-empty-prompt"));

    let config = AgentConfig::new("empty-prompt", "");
    let result = executor.run_agent(&config).await.unwrap();
    assert!(result.success, "dry-run with empty prompt should succeed");

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-empty-prompt");
}

#[test]
fn format_duration_minutes() {
    assert_eq!(
        execute::format_duration(Duration::from_secs(900)),
        "15m",
        "15 minutes"
    );
}

#[test]
fn format_duration_hours_and_minutes() {
    assert_eq!(
        execute::format_duration(Duration::from_secs(5400)),
        "1h30m",
        "1 hour 30 minutes"
    );
}

#[test]
fn format_duration_seconds_only() {
    assert_eq!(
        execute::format_duration(Duration::from_secs(45)),
        "45s",
        "45 seconds"
    );
}

#[test]
fn format_duration_zero() {
    assert_eq!(
        execute::format_duration(Duration::from_secs(0)),
        "0s",
        "zero"
    );
}

#[test]
fn format_duration_exact_hour() {
    assert_eq!(
        execute::format_duration(Duration::from_secs(3600)),
        "1h",
        "exact hour"
    );
}

#[test]
fn mutant_kill_format_duration_zero_vs_nonzero() {
    assert_eq!(
        execute::format_duration(Duration::ZERO),
        "0s",
        "zero duration must format as '0s'"
    );
    assert_eq!(
        execute::format_duration(Duration::from_secs(5)),
        "5s",
        "5 seconds must format as '5s'"
    );
    assert_eq!(
        execute::format_duration(Duration::from_secs(60)),
        "1m",
        "60 seconds must format as '1m'"
    );
    assert_eq!(
        execute::format_duration(Duration::from_secs(61)),
        "1m1s",
        "61 seconds must format as '1m1s'"
    );
    assert_eq!(
        execute::format_duration(Duration::from_secs(3600)),
        "1h",
        "3600 seconds must format as '1h'"
    );
    assert_eq!(
        execute::format_duration(Duration::from_secs(3601)),
        "1h",
        "3601 seconds formats as '1h' (seconds dropped in hour mode)"
    );
    assert_eq!(
        execute::format_duration(Duration::from_secs(3660)),
        "1h1m",
        "3660 seconds must format as '1h1m'"
    );
}

#[tokio::test]
async fn dry_run_with_tools_in_yaml() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-tools-config-test"));

    let config = AgentConfig {
        tools: Some(vec![String::from("Read"), String::from("Write")]),
        ..AgentConfig::new("test-tools", "test prompt")
    };

    let result = executor.run_agent(&config).await.unwrap();
    assert!(result.success, "dry-run should succeed");

    let task_yaml =
        std::fs::read_to_string("/tmp/pipelin3r-tools-config-test/test-tools/0/task.yaml").unwrap();
    assert!(
        task_yaml.contains("--allowedTools Read,Write"),
        "task YAML must contain tools, got: {task_yaml}"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-tools-config-test");
}

#[tokio::test]
async fn dry_run_with_retry_config_in_yaml() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-retry-config-test"));

    let config = AgentConfig {
        retry: Some(RetryConfig {
            max_retries: 5,
            initial_delay: Duration::from_secs(10),
            backoff_multiplier: 3.0,
            max_delay: Duration::from_secs(120),
        }),
        ..AgentConfig::new("test-retry", "test prompt")
    };

    let result = executor.run_agent(&config).await.unwrap();
    assert!(result.success, "dry-run should succeed");

    let task_yaml =
        std::fs::read_to_string("/tmp/pipelin3r-retry-config-test/test-retry/0/task.yaml").unwrap();
    assert!(
        task_yaml.contains("max-retries: 5"),
        "task YAML must contain custom max-retries, got: {task_yaml}"
    );
    assert!(
        task_yaml.contains("initial-delay: 10s"),
        "task YAML must contain custom initial-delay, got: {task_yaml}"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-retry-config-test");
}
