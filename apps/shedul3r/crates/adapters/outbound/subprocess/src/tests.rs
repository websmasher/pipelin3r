use super::*;

#[test]
fn test_format_duration_iso_whole_seconds() {
    let d = Duration::from_secs(30);
    assert_eq!(format_duration_iso(d), "PT30S", "whole seconds");
}

#[test]
fn test_format_duration_iso_fractional() {
    let d = Duration::from_millis(1500);
    assert_eq!(format_duration_iso(d), "PT1.5S", "1.5 seconds");
}

#[test]
fn test_format_duration_iso_small() {
    let d = Duration::from_millis(5);
    assert_eq!(format_duration_iso(d), "PT0.005S", "5 milliseconds");
}

#[test]
fn test_format_duration_iso_zero() {
    let d = Duration::from_secs(0);
    assert_eq!(format_duration_iso(d), "PT0S", "zero");
}

#[test]
fn test_strip_trailing_newline_lf() {
    assert_eq!(strip_trailing_newline("hello\n"), "hello", "strips LF");
}

#[test]
fn test_strip_trailing_newline_crlf() {
    assert_eq!(strip_trailing_newline("hello\r\n"), "hello", "strips CRLF");
}

#[test]
fn test_strip_trailing_newline_none() {
    assert_eq!(
        strip_trailing_newline("hello"),
        "hello",
        "no trailing newline"
    );
}

#[test]
fn test_strip_trailing_newline_empty() {
    assert_eq!(strip_trailing_newline(""), "", "empty string");
}

/// Asserts that no process matching the given pattern is running.
///
/// Uses `pgrep -f` to search. Skips the assertion if `pgrep` is not
/// available on the system.
fn assert_no_process(pattern: &str) {
    let output = std::process::Command::new("pgrep")
        .args(["-x", "sleep"])
        .output();
    // pgrep -x matches exact process name; we then check full args.
    // Fall back to checking via ps if pgrep gives a match.
    if let Ok(out) = output {
        if out.status.code() == Some(0) {
            // pgrep found sleep processes — verify none match our pattern.
            let ps_out = std::process::Command::new("ps")
                .args(["-eo", "args"])
                .output();
            if let Ok(ps) = ps_out {
                let text = String::from_utf8_lossy(&ps.stdout);
                assert!(
                    !text.lines().any(|l| l.contains(pattern)),
                    "{pattern} process should have been killed"
                );
            }
        }
    }
}

#[allow(clippy::unwrap_used)] // reason: test code, panics are acceptable
#[tokio::test]
async fn regression_subprocess_preserves_actual_exit_code() {
    // Regression: exit code was hardcoded to 1 regardless of the actual
    // process exit code. A command exiting with code 42 must report 42.
    let runner = TokioSubprocessRunner::new();
    let cmd = SubprocessCommand {
        command: vec!["/bin/sh".to_owned(), "-c".to_owned(), "exit 42".to_owned()],
        working_directory: None,
        environment: None,
        timeout: None,
        stdin_data: None,
    };

    let result = runner.run(cmd).await.unwrap();
    assert_eq!(
        result.exit_code, 42,
        "exit code must be 42, not hardcoded to 1"
    );
}

#[allow(clippy::unwrap_used)] // reason: test code, panics are acceptable
#[tokio::test]
async fn test_child_killed_on_cancel() {
    let marker = "sleep 54321";
    let runner = TokioSubprocessRunner::new();
    let cmd = SubprocessCommand {
        command: vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            format!("exec {marker}"),
        ],
        working_directory: None,
        environment: None,
        timeout: None,
        stdin_data: None,
    };

    // Start the task, then cancel it after a short delay.
    let handle = tokio::spawn(async move { runner.run(cmd).await });

    // Give the process time to start.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Cancel by aborting the task.
    handle.abort();
    let _ = handle.await; // JoinError::Cancelled

    // Give the OS time to clean up.
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_no_process(marker);
}

#[allow(clippy::unwrap_used)] // reason: test code, panics are acceptable
#[tokio::test]
async fn test_child_killed_on_timeout() {
    let marker = "sleep 54322";
    let runner = TokioSubprocessRunner::new();
    let cmd = SubprocessCommand {
        command: vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            format!("exec {marker}"),
        ],
        working_directory: None,
        environment: None,
        timeout: Some(Duration::from_millis(200)),
        stdin_data: None,
    };

    let result = runner.run(cmd).await;
    assert!(result.is_ok(), "timeout should return Ok with exit_code -1");
    let result = result.unwrap();
    assert_eq!(
        result.exit_code, -1,
        "timed out process should have exit code -1"
    );

    // Give the OS time to clean up.
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_no_process(marker);
}
