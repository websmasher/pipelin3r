#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

#[tokio::test]
async fn echo_command_succeeds() {
    let result = CommandBuilder::new("echo")
        .args(&["hello", "world"])
        .execute()
        .await;

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
    let result = CommandBuilder::new("false").execute().await;

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
