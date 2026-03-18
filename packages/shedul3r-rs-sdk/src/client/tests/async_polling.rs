//! Adversarial tests for async task polling (`submit_task_async`,
//! `get_task_status`, `submit_task_poll`).
//!
//! These tests use raw TCP mock servers to control exact HTTP responses and
//! timing, exposing edge cases in the polling loop.

#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(clippy::panic, reason = "test assertions")]
#![allow(
    clippy::disallowed_types,
    reason = "test code — std::sync::Mutex is fine here"
)]
#![allow(clippy::indexing_slicing, reason = "test code — panics are acceptable")]
#![allow(clippy::ignored_unit_patterns, reason = "test code")]
#![allow(clippy::needless_raw_string_hashes, reason = "test code — readability")]

use std::io::{Read as _, Write as _};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use super::super::*;
use crate::error::SdkError;

// ── Mock server helpers ─────────────────────────────────────────────

/// Type alias for mock server handle: `(bind address, join handle)`.
type MockHandle = (std::net::SocketAddr, std::thread::JoinHandle<()>);

/// Spawn a TCP mock that replies with fixed responses in sequence.
///
/// Each call to the mock consumes the next response from `responses`.
/// If there are more requests than responses, the last response is repeated.
fn spawn_sequential_mock(responses: Vec<String>) -> MockHandle {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let total = responses.len();
        for (i, _) in std::iter::repeat(()).enumerate().take(20) {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let idx = i.min(total.saturating_sub(1));
                let resp = responses.get(idx).map_or("", String::as_str);
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        }
    });
    (addr, handle)
}

/// Build an HTTP 200 response with a JSON body.
fn http_200(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Build an HTTP response with custom status.
fn http_response(status: u16, status_text: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Create a test client pointing at the given address with fast poll intervals.
fn test_client(addr: std::net::SocketAddr) -> Client {
    Client::new(ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: Duration::from_millis(2000),
        poll_interval: Duration::from_millis(50),
        poll_initial_delay: Duration::from_millis(10),
        max_poll_duration: Duration::from_millis(500),
    })
    .unwrap()
}

/// Standard test payload.
fn test_payload() -> TaskPayload {
    TaskPayload {
        task: String::from("name: test\ncommand: echo"),
        input: String::from("hello"),
        working_directory: None,
        environment: None,
        limiter_key: None,
        timeout_ms: None,
    }
}

// ── 1. submit_task_poll with server that never completes ────────────
//
// FIXED: submit_task_poll now respects max_poll_duration and returns
// SdkError::PollTimeout when the deadline is exceeded.

#[tokio::test]
async fn poll_never_completes_returns_poll_timeout() {
    // Server always returns "running" status.
    let running_body = r#"{"status":"running"}"#;
    let responses: Vec<String> = std::iter::once(
        // First request: async submit
        http_200(r#"{"task_id":"task-forever"}"#),
    )
    .chain(
        // All subsequent: running
        std::iter::repeat_n(http_200(running_body), 19),
    )
    .collect();

    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    // submit_task_poll now respects max_poll_duration (500ms in test_client).
    // It should return PollTimeout within a reasonable time.
    let result = tokio::time::timeout(
        Duration::from_millis(5000),
        client.submit_task_poll(&test_payload()),
    )
    .await;

    assert!(
        result.is_ok(),
        "submit_task_poll must terminate within 5s via its own max_poll_duration"
    );
    let inner = result.unwrap();
    assert!(
        inner.is_err(),
        "must return Err when server never completes"
    );
    assert!(
        matches!(inner, Err(SdkError::PollTimeout { .. })),
        "must be PollTimeout variant, got: {inner:?}"
    );
}

// ── 2. submit_task_poll: completed on first poll ────────────────────

#[tokio::test]
async fn poll_completed_on_first_poll_returns_immediately() {
    let responses = vec![
        // Async submit
        http_200(r#"{"task_id":"task-fast"}"#),
        // First status poll: completed
        http_200(
            r#"{"status":"completed","result":{"success":true,"output":"fast result","metadata":{"exit_code":0,"elapsed":0.5}}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let start = tokio::time::Instant::now();
    let result = client.submit_task_poll(&test_payload()).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "fast-completing task must succeed");
    let task_result = result.unwrap();
    assert!(task_result.success, "task must report success");
    assert_eq!(task_result.output, "fast result");
    // Should complete in roughly one poll_interval (50ms) + overhead, not
    // multiple cycles.
    assert!(
        elapsed < Duration::from_millis(300),
        "first-poll completion must be fast, took {elapsed:?}"
    );
}

// ── 3. submit_task_poll: server returns "failed" ────────────────────

#[tokio::test]
async fn poll_failed_status_returns_error_immediately() {
    let responses = vec![
        http_200(r#"{"task_id":"task-fail"}"#),
        http_200(r#"{"status":"failed","error":"process crashed"}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    assert!(result.is_err(), "failed task must return Err");
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert_eq!(
                message, "process crashed",
                "error message must be propagated from server"
            );
        }
        other => panic!("expected TaskFailed, got: {other:?}"),
    }
}

// ── 4. Poll interval timing ────────────────────────────────────────

#[tokio::test]
async fn poll_respects_interval_timing() {
    let poll_count = Arc::new(AtomicU32::new(0));
    let poll_count_clone = Arc::clone(&poll_count);

    // Server returns running 4 times, then completed on the 5th status poll.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let _handle = std::thread::spawn(move || {
        for i in 0..10_u32 {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let body = if i == 0 {
                    // async submit
                    String::from(r#"{"task_id":"task-timed"}"#)
                } else {
                    let _ = poll_count_clone.fetch_add(1, Ordering::SeqCst);
                    if i < 5 {
                        String::from(r#"{"status":"running"}"#)
                    } else {
                        String::from(
                            r#"{"status":"completed","result":{"success":true,"output":"done"}}"#,
                        )
                    }
                };
                let resp = http_200(&body);
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        }
    });

    let poll_interval = Duration::from_millis(80);
    let client = Client::new(ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: Duration::from_millis(5000),
        poll_interval,
        poll_initial_delay: Duration::from_millis(10),
        max_poll_duration: Duration::from_millis(5000),
    })
    .unwrap();

    let start = tokio::time::Instant::now();
    let result = client.submit_task_poll(&test_payload()).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "timed poll must succeed: {result:?}");

    let polls = poll_count.load(Ordering::SeqCst);
    assert!(
        polls >= 4,
        "must have polled at least 4 times (running) + 1 (completed), got {polls}"
    );

    // With 5 polls at 80ms each, minimum time is ~400ms.
    // Allow generous margin but ensure it's not instant (which would mean
    // poll_interval is being ignored).
    let min_expected = Duration::from_millis(300);
    assert!(
        elapsed >= min_expected,
        "elapsed {elapsed:?} is less than {min_expected:?}; \
         poll_interval may not be respected"
    );
}

// ── 5. submit_task_async: server returns 500 ────────────────────────

#[tokio::test]
async fn async_submit_server_error_propagates() {
    let responses = vec![http_response(
        500,
        "Internal Server Error",
        r#"{"error":"database connection failed"}"#,
    )];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_async(&test_payload()).await;

    // FIXED: The SDK now explicitly checks for non-2xx status before parsing.
    assert!(result.is_err(), "500 from async submit must return Err");
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert!(
                message.contains("500"),
                "error must mention HTTP 500: {message}"
            );
        }
        other => panic!("expected TaskFailed, got: {other:?}"),
    }
}

// ── 6. get_task_status: server returns 500 mid-polling ──────────────
//
// FIXED: get_task_status now explicitly checks for non-2xx/non-404
// status codes. The transient retry in submit_task_poll retries 5xx
// errors up to 3 times. This mock only returns one 500, so after
// 3 retries (all hitting the last response) it gives up.

#[tokio::test]
async fn status_poll_server_error_mid_polling_propagates() {
    let responses = vec![
        // Async submit: OK
        http_200(r#"{"task_id":"task-mid500"}"#),
        // First poll: running
        http_200(r#"{"status":"running"}"#),
        // Subsequent polls: 500 error (mock repeats last response)
        http_response(
            500,
            "Internal Server Error",
            r#"{"error":"transient failure"}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    assert!(
        result.is_err(),
        "500 during polling must cause an error after retries are exhausted"
    );
}

// ── 7. Completed response with empty/missing result field ───────────

#[tokio::test]
async fn completed_with_no_result_field_returns_empty_output() {
    let responses = vec![
        http_200(r#"{"task_id":"task-empty"}"#),
        http_200(r#"{"status":"completed"}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    // Code path: `status.result` is None, returns TaskResult with empty output.
    assert!(result.is_ok(), "completed with no result must succeed");
    let task_result = result.unwrap();
    assert!(
        task_result.success,
        "completed with no result should be treated as success"
    );
    assert_eq!(
        task_result.output, "",
        "output should be empty when result is None"
    );
}

// ── 8. Elapsed format variants via untagged enum ────────────────────

#[tokio::test]
async fn completed_with_float_elapsed_format() {
    let responses = vec![
        http_200(r#"{"task_id":"task-float-elapsed"}"#),
        http_200(
            r#"{"status":"completed","result":{"success":true,"output":"ok","metadata":{"elapsed":42.5,"exit_code":0}}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok(), "float elapsed must parse: {result:?}");
    let tr = result.unwrap();
    assert!(tr.elapsed.is_some(), "elapsed must be populated");
    let elapsed = tr.elapsed.unwrap();
    assert!(
        elapsed.contains("42"),
        "elapsed display must contain 42: {elapsed}"
    );
}

#[tokio::test]
async fn completed_with_struct_elapsed_format() {
    let responses = vec![
        http_200(r#"{"task_id":"task-struct-elapsed"}"#),
        http_200(
            r#"{"status":"completed","result":{"success":true,"output":"ok","metadata":{"elapsed":{"secs":10,"nanos":250000000},"exit_code":0}}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok(), "struct elapsed must parse: {result:?}");
    let tr = result.unwrap();
    assert!(tr.elapsed.is_some(), "elapsed must be populated");
    let elapsed = tr.elapsed.unwrap();
    assert!(
        elapsed.contains("10"),
        "elapsed display must contain 10: {elapsed}"
    );
}

// ── 9. Unknown status string ────────────────────────────────────────

#[tokio::test]
async fn unknown_status_string_returns_error() {
    let responses = vec![
        http_200(r#"{"task_id":"task-cancelled"}"#),
        http_200(r#"{"status":"cancelled"}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    assert!(
        result.is_err(),
        "unknown status 'cancelled' must return Err"
    );
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert!(
                message.contains("cancelled"),
                "error message must mention the unknown status: {message}"
            );
        }
        other => panic!("expected TaskFailed with 'cancelled', got: {other:?}"),
    }
}

// ── 10. Malformed JSON in status response ───────────────────────────

#[tokio::test]
async fn malformed_json_in_status_response_returns_error() {
    let responses = vec![
        http_200(r#"{"task_id":"task-bad-json"}"#),
        // Malformed JSON as status response
        http_200("this is not json at all {{{"),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    assert!(
        result.is_err(),
        "malformed JSON status response must return Err"
    );
}

// ── 11. submit_task_async with malformed JSON submit response ───────

#[tokio::test]
async fn async_submit_malformed_json_returns_error() {
    let responses = vec![http_200("not valid json")];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_async(&test_payload()).await;

    assert!(
        result.is_err(),
        "malformed JSON from async submit must return Err"
    );
    assert!(
        matches!(result, Err(SdkError::TaskFailed { .. })),
        "must be TaskFailed variant for parse error"
    );
}

// ── 12. get_task_status with 404 returns TaskFailed ─────────────────

#[tokio::test]
async fn get_task_status_404_returns_task_not_found() {
    let responses = vec![http_response(404, "Not Found", r#"{"error":"not found"}"#)];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.get_task_status("nonexistent-task-id").await;

    assert!(result.is_err(), "404 must return Err");
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert!(
                message.contains("nonexistent-task-id"),
                "error must include the task ID: {message}"
            );
        }
        other => panic!("expected TaskFailed, got: {other:?}"),
    }
}

// ── 13. Failed task with no error field uses default message ────────

#[tokio::test]
async fn failed_status_no_error_field_uses_default() {
    let responses = vec![
        http_200(r#"{"task_id":"task-fail-no-msg"}"#),
        http_200(r#"{"status":"failed"}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    assert!(result.is_err(), "failed status must return Err");
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert_eq!(
                message, "unknown error",
                "missing error field must produce 'unknown error'"
            );
        }
        other => panic!("expected TaskFailed, got: {other:?}"),
    }
}

// ── 14. Task ID with special characters in URL ─────────────────────

#[tokio::test]
async fn task_id_with_special_chars_in_url() {
    // Test that get_task_status doesn't URL-encode the task_id,
    // which would break if the server expects exact IDs.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let received_path = Arc::new(std::sync::Mutex::new(String::new()));
    let received_path_clone = Arc::clone(&received_path);

    let _handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            if let Ok(n) = stream.read(&mut buf) {
                let request = String::from_utf8_lossy(&buf[..n]);
                // Extract the path from "GET /path HTTP/1.1"
                if let Some(first_line) = request.lines().next() {
                    let parts: Vec<&str> = first_line.split_whitespace().collect();
                    if let Some(path) = parts.get(1) {
                        let mut guard = received_path_clone.lock().unwrap();
                        *guard = (*path).to_owned();
                    }
                }
            }
            let body = r#"{"status":"completed","result":{"success":true,"output":"ok"}}"#;
            let resp = http_200(body);
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });

    let client = test_client(addr);

    // Task ID with characters that would need URL encoding
    let task_id = "task/with spaces&special=chars";
    let _result = client.get_task_status(task_id).await;

    // Check what path was actually sent. The task_id is interpolated
    // directly into the URL without encoding, which could cause issues.
    let path = received_path.lock().unwrap().clone();
    // The URL format!("{}/api/tasks/async/{task_id}", ...) will produce
    // a path with raw spaces/slashes. reqwest may or may not normalize this.
    // This test documents the behavior.
    assert!(
        !path.is_empty(),
        "should have received a request (path: {path})"
    );
}

// ── 15. Completed result with success=false ─────────────────────────

#[tokio::test]
async fn completed_with_success_false_in_result() {
    // The server says status="completed" but the result has success=false.
    // This can happen when the command ran but returned non-zero exit code.
    let responses = vec![
        http_200(r#"{"task_id":"task-completed-fail"}"#),
        http_200(
            r#"{"status":"completed","result":{"success":false,"output":"exit code 1","metadata":{"exit_code":1}}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;

    // The status is "completed" so submit_task_poll returns Ok, but
    // TaskResult.success will be false.
    assert!(
        result.is_ok(),
        "completed status must return Ok even if result.success=false"
    );
    let tr = result.unwrap();
    assert!(
        !tr.success,
        "TaskResult.success must be false when result says success=false"
    );
    assert_eq!(tr.output, "exit code 1");
    assert_eq!(tr.exit_code, Some(1));
}

// ── 16. Multiple running polls then completion ──────────────────────

#[tokio::test]
async fn multiple_running_polls_then_completion() {
    let responses = vec![
        http_200(r#"{"task_id":"task-multi-poll"}"#),
        http_200(r#"{"status":"running"}"#),
        http_200(r#"{"status":"running"}"#),
        http_200(r#"{"status":"running"}"#),
        http_200(r#"{"status":"pending"}"#),
        http_200(r#"{"status":"completed","result":{"success":true,"output":"finally done"}}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok(), "must eventually succeed: {result:?}");
    let tr = result.unwrap();
    assert!(tr.success);
    assert_eq!(tr.output, "finally done");
}

// ── 17. Pending status is also treated as "continue polling" ────────

#[tokio::test]
async fn pending_status_continues_polling() {
    let responses = vec![
        http_200(r#"{"task_id":"task-pending"}"#),
        http_200(r#"{"status":"pending"}"#),
        http_200(r#"{"status":"completed","result":{"success":true,"output":"after pending"}}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok(), "pending->completed must succeed");
    let tr = result.unwrap();
    assert_eq!(tr.output, "after pending");
}

// ── 18. submit_task_async returns task_id correctly ─────────────────

#[tokio::test]
async fn async_submit_returns_task_id() {
    let responses = vec![http_200(r#"{"task_id":"abc-123-def"}"#)];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_async(&test_payload()).await;
    assert!(result.is_ok(), "valid submit must return Ok");
    assert_eq!(result.unwrap(), "abc-123-def");
}

// ── 19. Completed with null output in result ────────────────────────

#[tokio::test]
async fn completed_with_null_output_in_result() {
    let responses = vec![
        http_200(r#"{"task_id":"task-null-output"}"#),
        http_200(
            r#"{"status":"completed","result":{"success":true,"output":null,"metadata":null}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok(), "null output must not crash: {result:?}");
    let tr = result.unwrap();
    assert!(tr.success);
    // output is Option<String> = None, unwrap_or_default -> ""
    assert_eq!(tr.output, "");
}

// ── 20. Deserialization: result with message but no output ──────────

#[tokio::test]
async fn completed_result_uses_message_as_fallback() {
    // When success=false, output=None, message="some error"
    let responses = vec![
        http_200(r#"{"task_id":"task-msg-fallback"}"#),
        http_200(r#"{"status":"completed","result":{"success":false,"message":"fallback error"}}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok(), "completed must return Ok");
    let tr = result.unwrap();
    assert!(!tr.success);
    assert_eq!(
        tr.output, "fallback error",
        "must fall back to 'message' when 'output' is missing"
    );
}

// ── 21. get_task_status HTTP 500 returns explicit error ──────────────

#[tokio::test]
async fn get_task_status_500_returns_error() {
    let responses = vec![http_response(
        500,
        "Internal Server Error",
        r#"<html>Server Error</html>"#,
    )];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.get_task_status("some-task").await;

    // FIXED: get_task_status now explicitly checks for non-2xx status codes.
    assert!(
        result.is_err(),
        "500 with HTML body must return Err: {result:?}"
    );
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert!(
                message.contains("500"),
                "error must mention HTTP 500: {message}"
            );
        }
        other => panic!("expected TaskFailed, got: {other:?}"),
    }
}

// ── 22. submit_task_async with HTTP 500 status code ─────────────────
//
// FIXED: submit_task_async now checks the HTTP status code before
// parsing the body. A 500 response returns Err even if the body
// contains a valid task_id.

#[tokio::test]
async fn async_submit_500_with_task_id_body_returns_error() {
    let responses = vec![http_response(
        500,
        "Internal Server Error",
        r#"{"task_id":"should-not-be-accepted"}"#,
    )];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_async(&test_payload()).await;

    assert!(
        result.is_err(),
        "500 response must return Err even if body has task_id"
    );
    match result {
        Err(SdkError::TaskFailed { message }) => {
            assert!(
                message.contains("500"),
                "error message must mention HTTP 500: {message}"
            );
        }
        other => panic!("expected TaskFailed, got: {other:?}"),
    }
}

// ── 23. Rapid sequence: submit -> immediate complete ────────────────

#[tokio::test]
async fn submit_and_poll_completes_with_full_metadata() {
    let responses = vec![
        http_200(r#"{"task_id":"task-full-meta"}"#),
        http_200(
            r#"{"status":"completed","result":{"success":true,"output":"  hello world  ","metadata":{"exit_code":0,"started_at":"2025-06-01T12:00:00Z","elapsed":1.234}}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    assert!(result.is_ok());
    let tr = result.unwrap();
    assert!(tr.success);
    // Note: output is trimmed
    assert_eq!(tr.output, "hello world");
    assert_eq!(tr.exit_code, Some(0));
    assert_eq!(tr.started_at.as_deref(), Some("2025-06-01T12:00:00Z"));
    assert!(tr.elapsed.is_some());
}

// ── 24. Empty task_id from server ───────────────────────────────────

#[tokio::test]
async fn async_submit_empty_task_id() {
    let responses = vec![
        // Submit returns empty task_id
        http_200(r#"{"task_id":""}"#),
        // Status poll for empty string — server returns completed
        http_200(r#"{"status":"completed","result":{"success":true,"output":"empty id ok"}}"#),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    // The SDK should accept an empty task_id (it's just a string).
    // The URL would be /api/tasks/ which might cause server-side issues
    // but the SDK shouldn't crash.
    let result = client.submit_task_poll(&test_payload()).await;
    // This should work since the mock doesn't care about the URL path.
    assert!(
        result.is_ok(),
        "empty task_id must not crash SDK: {result:?}"
    );
}

// ── 25. Response with extra unknown fields ──────────────────────────

#[tokio::test]
async fn status_response_with_extra_fields_still_parses() {
    let responses = vec![
        http_200(r#"{"task_id":"task-extra","unknown_field":"ignored"}"#),
        http_200(
            r#"{"status":"completed","result":{"success":true,"output":"ok"},"extra_field":42,"nested":{"a":1}}"#,
        ),
    ];
    let (addr, _handle) = spawn_sequential_mock(responses);
    let client = test_client(addr);

    let result = client.submit_task_poll(&test_payload()).await;
    // serde's default is to ignore unknown fields, so this should work.
    assert!(
        result.is_ok(),
        "extra fields in response must be ignored: {result:?}"
    );
}
