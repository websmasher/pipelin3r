#![allow(clippy::unwrap_used, reason = "test assertions")]

mod regression;

use std::time::Duration;

use super::*;
use crate::error::SdkError;

#[test]
fn truncate_str_short_passthrough() {
    let s = "hello";
    assert_eq!(truncate_str(s, 10), "hello", "short strings pass through");
}

#[test]
fn truncate_str_exact_boundary() {
    let s = "abcde";
    assert_eq!(
        truncate_str(s, 5),
        "abcde",
        "strings at exact boundary pass through"
    );
}

#[test]
fn truncate_str_cuts_long() {
    let s = "abcdefghij";
    assert_eq!(truncate_str(s, 5), "abcde", "should truncate to max_len");
}

#[test]
fn truncate_str_multibyte_boundary() {
    // Each emoji is 4 bytes. "ab" = 2 bytes, then 4-byte char.
    let s = "ab\u{1F600}cd";
    // max_len=4 lands inside the emoji — should back up to byte 2.
    assert_eq!(truncate_str(s, 4), "ab", "should back up to char boundary");
}

#[test]
fn truncate_str_zero_max() {
    let s = "hello";
    assert_eq!(truncate_str(s, 0), "", "zero max_len produces empty string");
}

#[test]
fn client_config_default_values() {
    let cfg = ClientConfig::default();
    assert_eq!(cfg.base_url, "http://localhost:7943", "default base URL");
    assert_eq!(
        cfg.timeout,
        Duration::from_millis(2_700_000),
        "default timeout is 45 minutes"
    );
    assert_eq!(
        cfg.poll_interval,
        Duration::from_millis(10_000),
        "default poll interval is 10 seconds"
    );
    assert_eq!(
        cfg.poll_initial_delay,
        Duration::from_millis(30_000),
        "default poll initial delay is 30 seconds"
    );
    assert_eq!(
        cfg.max_poll_duration,
        Duration::from_millis(2_700_000),
        "default max poll duration is 45 minutes"
    );
}

#[test]
fn client_config_custom() {
    let cfg = ClientConfig {
        base_url: String::from("http://example.com:8080"),
        timeout: Duration::from_secs(60),
        poll_interval: Duration::from_secs(1),
        poll_initial_delay: Duration::from_secs(5),
        max_poll_duration: Duration::from_secs(120),
    };
    assert_eq!(cfg.base_url, "http://example.com:8080", "custom base URL");
    assert_eq!(cfg.timeout, Duration::from_secs(60), "custom timeout");
    assert_eq!(
        cfg.max_poll_duration,
        Duration::from_secs(120),
        "custom max poll duration"
    );
}

#[tokio::test]
async fn poll_for_file_timeout_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.txt");

    let result = poll_for_file(
        &path,
        Duration::from_millis(10),
        Duration::from_millis(10),
        Duration::from_millis(50),
    )
    .await;

    assert!(result.is_err(), "should timeout when file does not appear");
    let is_poll_timeout = matches!(result, Err(SdkError::PollTimeout { .. }));
    assert!(is_poll_timeout, "should be PollTimeout variant");
}

#[tokio::test]
async fn poll_for_file_succeeds_when_file_appears() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.txt");
    let path_clone = path.clone();

    // Spawn a task that creates the file after 100ms.
    let _handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        #[allow(clippy::disallowed_methods)] // test code: simulating file system state
        let _ = std::fs::write(&path_clone, "done");
    });

    let result = poll_for_file(
        &path,
        Duration::from_millis(10),
        Duration::from_millis(50),
        Duration::from_millis(500),
    )
    .await;

    assert!(
        result.is_ok(),
        "should succeed when file appears during polling"
    );
}

#[tokio::test]
async fn submit_task_returns_err_on_connection_refused() {
    let config = ClientConfig {
        base_url: String::from("http://127.0.0.1:19999"),
        timeout: Duration::from_millis(500),
        ..ClientConfig::default()
    };
    let client = Client::new(config).unwrap();

    let payload = TaskPayload {
        task: String::from("name: test\ncommand: echo"),
        input: String::from("hello"),
        working_directory: None,
        environment: None,
        limiter_key: None,
        timeout_ms: None,
    };
    let result = client.submit_task(&payload).await;

    assert!(result.is_err(), "network failure should return Err");
    let is_http = matches!(result, Err(SdkError::Http(_)));
    assert!(is_http, "should be SdkError::Http variant");
}

#[tokio::test]
async fn submit_task_with_recovery_recovers_from_file() {
    // Bind a listener that accepts connections but never responds,
    // so the HTTP call hangs rather than failing immediately.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    // Accept connections in background so the OS doesn't RST them.
    let _accept_handle = tokio::spawn(async move {
        // Keep listener alive and accept (but never read/write).
        loop {
            let _conn = listener.accept();
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("recovered.txt");
    let output_clone = output_path.clone();

    // Spawn a task that creates the file after 50ms.
    let _handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        #[allow(clippy::disallowed_methods)] // test code: simulating file system state
        let _ = std::fs::write(&output_clone, "recovered content");
    });

    let config = ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: Duration::from_millis(5000),
        poll_interval: Duration::from_millis(30),
        poll_initial_delay: Duration::from_millis(10),
        max_poll_duration: Duration::from_millis(1000),
    };
    let client = Client::new(config).unwrap();

    let payload = TaskPayload {
        task: String::from("name: test\ncommand: echo"),
        input: String::from("hello"),
        working_directory: None,
        environment: None,
        limiter_key: None,
        timeout_ms: None,
    };

    let result = client
        .submit_task_with_recovery(&payload, &output_path)
        .await;

    assert!(result.is_ok(), "should recover via file poll");
    let task_result = result.unwrap();
    assert!(task_result.success, "recovered result should be successful");
}

#[test]
fn task_result_require_success_ok() {
    let result = TaskResult {
        success: true,
        output: String::from("done"),
        exit_code: Some(0),
        elapsed: None,
        started_at: None,
    };
    assert!(
        result.require_success().is_ok(),
        "should pass for successful result"
    );
}

#[test]
fn task_result_require_success_err() {
    let result = TaskResult {
        success: false,
        output: String::from("command failed"),
        exit_code: Some(1),
        elapsed: None,
        started_at: None,
    };
    let err = result.require_success();
    assert!(err.is_err(), "should fail for unsuccessful result");
    let is_task_failed = matches!(err, Err(SdkError::TaskFailed { .. }));
    assert!(is_task_failed, "should be TaskFailed variant");
}

#[test]
fn api_elapsed_display_seconds_only() {
    let elapsed = ApiElapsed::Float(42.0);
    assert_eq!(elapsed.to_display_string(), "42s");
}

#[test]
fn api_elapsed_display_with_millis() {
    let elapsed = ApiElapsed::Float(3.150);
    // Float precision: 3.150 → whole=3, frac=0.150*1000=150
    let display = elapsed.to_display_string();
    assert!(display.starts_with("3."), "should start with 3.: {display}");
    assert!(display.ends_with('s'), "should end with s: {display}");
}

#[test]
fn api_elapsed_display_legacy_struct() {
    let elapsed = ApiElapsed::Struct {
        secs: Some(3),
        nanos: Some(150_000_000),
    };
    assert_eq!(elapsed.to_display_string(), "3.150s");
}
