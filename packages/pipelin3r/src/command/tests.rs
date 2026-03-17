#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

#[tokio::test]
async fn echo_command_succeeds() {
    let config = CommandConfig {
        args: vec![String::from("hello"), String::from("world")],
        ..CommandConfig::new("echo")
    };

    let result = run_command(&config).await;

    assert!(result.is_ok(), "echo should not fail to spawn");
    let cmd_result = result.unwrap_or_else(|_| CommandResult {
        success: false,
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
    });
    assert!(cmd_result.success, "echo should succeed");
    assert_eq!(
        cmd_result.stdout.trim(),
        "hello world",
        "echo output should match"
    );
    assert_eq!(cmd_result.exit_code, Some(0), "exit code should be 0");
}

#[tokio::test]
async fn false_command_fails() {
    let config = CommandConfig::new("false");
    let result = run_command(&config).await;

    assert!(result.is_ok(), "false should not fail to spawn");
    let cmd_result = result.unwrap_or_else(|_| CommandResult {
        success: true,
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
    });
    assert!(!cmd_result.success, "false should report failure");
}

#[test]
fn require_success_on_success() {
    let result = CommandResult {
        success: true,
        stdout: String::from("ok"),
        stderr: String::new(),
        exit_code: Some(0),
    };
    assert!(
        result.require_success().is_ok(),
        "should return Ok for successful command"
    );
}

#[test]
fn require_success_on_failure() {
    let result = CommandResult {
        success: false,
        stdout: String::new(),
        stderr: String::from("something went wrong"),
        exit_code: Some(1),
    };
    let err = result.require_success();
    assert!(err.is_err(), "should return Err for failed command");
}

#[tokio::test]
async fn command_with_env_vars() {
    let mut env = BTreeMap::new();
    let _ = env.insert(String::from("TEST_VAR"), String::from("hello_env"));

    let config = CommandConfig {
        args: vec![String::from("-c"), String::from("echo $TEST_VAR")],
        env: Some(env),
        ..CommandConfig::new("sh")
    };

    let result = run_command(&config).await;
    assert!(result.is_ok(), "sh should not fail to spawn");
    let cmd_result = result.unwrap_or_else(|_| CommandResult {
        success: false,
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
    });
    assert!(cmd_result.success, "sh -c echo should succeed");
    assert_eq!(
        cmd_result.stdout.trim(),
        "hello_env",
        "env var should be passed through"
    );
}

#[tokio::test]
async fn command_timeout_expires() {
    let config = CommandConfig {
        args: vec![String::from("-c"), String::from("sleep 60")],
        timeout: Some(Duration::from_millis(50)),
        ..CommandConfig::new("sh")
    };

    let result = run_command(&config).await;
    assert!(result.is_err(), "command should fail due to timeout");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("timed out"),
        "error should mention timeout, got: {err_msg}"
    );
}
