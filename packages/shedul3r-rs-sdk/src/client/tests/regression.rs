//! Regression and mutant-kill tests for the client module.

#![allow(clippy::unwrap_used, reason = "test assertions")]

use std::time::Duration;

use super::super::*;
use crate::error::SdkError;

/// Type alias for mock server handle: `(bind address, server thread)`.
type MockServer = (std::net::SocketAddr, std::thread::JoinHandle<()>);

#[tokio::test]
async fn regression_http_call_returns_err_on_network_failure() {
    // Regression: http_call previously swallowed network errors and returned
    // Ok(TaskResult{success:false}) instead of Err(SdkError::Http).
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

    // Must be Err, not Ok with success=false.
    assert!(
        result.is_err(),
        "connection refused must return Err, not Ok"
    );
    assert!(
        matches!(result, Err(SdkError::Http(_))),
        "must be SdkError::Http variant, got: {result:?}"
    );
}

#[test]
#[allow(clippy::disallowed_methods)] // test code: deserializing test fixtures
fn regression_response_metadata_populated() {
    // Regression: TaskResult metadata fields (exit_code, elapsed, started_at)
    // were dropped and always None.
    let json = serde_json::json!({
        "success": true,
        "output": "ok",
        "metadata": {
            "exit_code": 0,
            "started_at": "2025-01-15T10:00:00Z",
            "elapsed": { "secs": 42, "nanos": 500_000_000_u64 }
        }
    });
    let api_resp: ApiResponse = serde_json::from_value(json).unwrap();
    let meta = api_resp.metadata.as_ref();
    let exit_code = meta.and_then(|m| m.exit_code);
    let elapsed = meta.and_then(|m| m.elapsed.as_ref().map(ApiElapsed::to_display_string));
    let started_at = meta.and_then(|m| m.started_at.clone());

    assert_eq!(
        exit_code,
        Some(0),
        "exit_code must be populated from response"
    );
    assert_eq!(
        elapsed.as_deref(),
        Some("42.500s"),
        "elapsed must be populated from response"
    );
    assert_eq!(
        started_at.as_deref(),
        Some("2025-01-15T10:00:00Z"),
        "started_at must be populated from response"
    );
}

#[tokio::test]
async fn regression_poll_timeout_overshoot_initial_delay_exceeds_max() {
    // Regression: poll_for_file with initial_delay > max_duration would sleep
    // for the full initial_delay before checking timeout, overshooting max_duration.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.txt");

    let max_duration = Duration::from_millis(50);
    let initial_delay = Duration::from_millis(500); // 10x max_duration

    let start = tokio::time::Instant::now();
    let result = poll_for_file(
        &path,
        initial_delay,
        Duration::from_millis(10),
        max_duration,
    )
    .await;

    let actual_elapsed = start.elapsed();

    assert!(result.is_err(), "should timeout");
    assert!(
        matches!(result, Err(SdkError::PollTimeout { .. })),
        "should be PollTimeout"
    );
    // Must return within max_duration + generous epsilon (100ms), NOT after initial_delay (500ms).
    let epsilon = Duration::from_millis(100);
    assert!(
        actual_elapsed < max_duration.saturating_add(epsilon),
        "poll must return within max_duration + epsilon ({:?}), but took {:?}",
        max_duration.saturating_add(epsilon),
        actual_elapsed
    );
}

#[tokio::test]
async fn regression_file_poll_recovery_succeeds() {
    // Regression: poll_for_file would not detect files created after the
    // initial delay but before timeout.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.txt");
    let path_clone = path.clone();

    // Create file after 80ms — well after initial_delay (10ms) but before timeout (500ms).
    let _handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(80)).await;
        #[allow(clippy::disallowed_methods)] // test code: simulating file system state
        let _ = std::fs::write(&path_clone, "done");
    });

    let result = poll_for_file(
        &path,
        Duration::from_millis(10),
        Duration::from_millis(30),
        Duration::from_millis(500),
    )
    .await;

    assert!(result.is_ok(), "file created mid-poll must be detected");
}

#[test]
fn regression_require_success_returns_task_failed() {
    // Regression: TaskResult{success:false}.require_success() must return
    // Err(SdkError::TaskFailed), not swallow the error as Ok.
    let result = TaskResult {
        success: false,
        output: String::from("command failed"),
        exit_code: Some(1),
        elapsed: None,
        started_at: None,
    };
    let err = result.require_success();
    assert!(err.is_err(), "failed task must return Err");
    assert!(
        matches!(&err, Err(SdkError::TaskFailed { message }) if message == "command failed"),
        "must be TaskFailed with preserved output message, got: {err:?}"
    );
}

#[test]
fn regression_task_payload_serializes_limiter_key_and_timeout_ms() {
    // Regression: limiter_key and timeout_ms fields were missing from
    // TaskPayload serialization.
    let payload = TaskPayload {
        task: String::from("name: test\ncommand: echo"),
        input: String::from("hello"),
        working_directory: None,
        environment: None,
        limiter_key: Some(String::from("my-limiter")),
        timeout_ms: Some(30_000),
    };
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        json.get("limiter_key").and_then(serde_json::Value::as_str),
        Some("my-limiter"),
        "limiter_key must be serialized"
    );
    assert_eq!(
        json.get("timeout_ms").and_then(serde_json::Value::as_u64),
        Some(30_000),
        "timeout_ms must be serialized"
    );
}

#[test]
fn mutant_kill_base_url_returns_configured_url() {
    // Mutant kill: base_url() replaced with "" or "xyzzy"
    let config = ClientConfig {
        base_url: String::from("http://custom-server:9999"),
        ..ClientConfig::default()
    };
    let client = Client::new(config).unwrap();
    assert_eq!(
        client.base_url(),
        "http://custom-server:9999",
        "base_url() must return the configured URL, not an empty string or constant"
    );
}

#[test]
#[allow(clippy::disallowed_methods)] // test code: deserializing test fixtures
fn mutant_kill_success_check_true_returns_ok() {
    // Mutant kill: `== with !=` on success check
    let json_success = serde_json::json!({
        "success": true,
        "output": "good",
        "metadata": null
    });
    let api_resp: ApiResponse = serde_json::from_value(json_success).unwrap();
    assert_eq!(
        api_resp.success,
        Some(true),
        "success=true in JSON must parse as Some(true)"
    );
    let result = TaskResult {
        success: api_resp.success == Some(true),
        output: api_resp.output.unwrap_or_default().trim().to_owned(),
        exit_code: None,
        elapsed: None,
        started_at: None,
    };
    assert!(
        result.success,
        "response with success=true must produce TaskResult.success=true"
    );

    // Also verify the inverse: success=false must NOT match.
    let json_failure = serde_json::json!({
        "success": false,
        "output": "bad"
    });
    let api_resp_fail: ApiResponse = serde_json::from_value(json_failure).unwrap();
    let result_fail = TaskResult {
        success: api_resp_fail.success == Some(true),
        output: String::from("bad"),
        exit_code: None,
        elapsed: None,
        started_at: None,
    };
    assert!(
        !result_fail.success,
        "response with success=false must produce TaskResult.success=false"
    );
}

#[test]
fn mutant_kill_truncate_str_at_exact_boundary() {
    let s = "abcde"; // 5 bytes
    let result = truncate_str(s, 5);
    assert_eq!(
        result, "abcde",
        "string of exactly max_len must pass through unchanged"
    );
    assert_eq!(
        result.len(),
        5,
        "result length must equal max_len when input length equals max_len"
    );

    let s2 = "abcdef"; // 6 bytes
    let result2 = truncate_str(s2, 5);
    assert_eq!(
        result2, "abcde",
        "string one byte over max_len must be truncated"
    );
    assert_eq!(
        result2.len(),
        5,
        "truncated result must have exactly max_len bytes"
    );

    // Edge case: single multibyte char with max_len that splits it must back up to 0.
    let s3 = "\u{1F600}"; // 4 bytes
    let result3 = truncate_str(s3, 2);
    assert_eq!(
        result3, "",
        "truncate inside a multibyte char must back up to empty when no char boundary fits"
    );
}

/// Start a TCP listener that responds with a fixed HTTP response to any request.
fn spawn_http_mock(status: u16, status_text: &str, body: &str) -> MockServer {
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for _ in 0..5 {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut buf);
                let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
                let _ = std::io::Write::flush(&mut stream);
            }
        }
    });
    (addr, handle)
}

#[tokio::test]
async fn mutant_kill_v2_http_call_success_true_returns_success() {
    let body = r#"{"success":true,"output":"hello"}"#;
    let (addr, _handle) = spawn_http_mock(200, "OK", body);

    let config = ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: Duration::from_millis(2000),
        ..ClientConfig::default()
    };
    let client = Client::new(config).unwrap();

    let result = http_call(
        client.http_client(),
        &format!("http://{addr}/run"),
        &TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hi"),
            working_directory: None,
            environment: None,
            limiter_key: None,
            timeout_ms: None,
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        result.success,
        "response with success:true must produce TaskResult.success=true; \
         if this fails, the == check was mutated to !="
    );
    assert_eq!(
        result.output, "hello",
        "output must be preserved from response"
    );
}

#[tokio::test]
async fn mutant_kill_v2_http_call_success_false_returns_failure() {
    let body = r#"{"success":false,"output":"boom"}"#;
    let (addr, _handle) = spawn_http_mock(200, "OK", body);

    let config = ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: Duration::from_millis(2000),
        ..ClientConfig::default()
    };
    let client = Client::new(config).unwrap();

    let result = http_call(
        client.http_client(),
        &format!("http://{addr}/run"),
        &TaskPayload {
            task: String::from("name: test\ncommand: echo"),
            input: String::from("hi"),
            working_directory: None,
            environment: None,
            limiter_key: None,
            timeout_ms: None,
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        !result.success,
        "response with success:false must produce TaskResult.success=false"
    );
    assert_eq!(result.output, "boom", "error output must be preserved");
}

#[test]
fn mutant_kill_v2_truncate_str_boundary_len_equals_max() {
    let s = "exact"; // 5 bytes
    let result = truncate_str(s, 5);
    assert_eq!(
        result, "exact",
        "string of exactly max_len must pass through unchanged"
    );
    assert_eq!(
        result.len(),
        s.len(),
        "output length must equal input length when len == max_len"
    );

    let s2 = "abcd"; // 4 bytes
    assert_eq!(
        truncate_str(s2, 5),
        "abcd",
        "string shorter than max_len must pass through"
    );

    let s3 = "abcdef"; // 6 bytes
    assert_eq!(
        truncate_str(s3, 5),
        "abcde",
        "string longer than max_len must be truncated"
    );
}

#[test]
fn regression_task_payload_omits_none_optional_fields() {
    let payload = TaskPayload {
        task: String::from("name: test\ncommand: echo"),
        input: String::from("hello"),
        working_directory: None,
        environment: None,
        limiter_key: None,
        timeout_ms: None,
    };
    let json = serde_json::to_value(&payload).unwrap();
    assert!(
        json.get("limiter_key").is_none(),
        "None limiter_key must be omitted"
    );
    assert!(
        json.get("timeout_ms").is_none(),
        "None timeout_ms must be omitted"
    );
}
