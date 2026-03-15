//! Outbound adapter for subprocess execution via `tokio::process`.
//!
//! Provides [`TokioSubprocessRunner`], a concrete implementation of the
//! [`SubprocessRunner`](repo::SubprocessRunner) port trait that spawns shell
//! commands as child processes using Tokio's async process API.

use std::process::Stdio;
use std::time::Duration;

use domain_types::{EnvironmentMap, SchedulrError, SubprocessCommand, SubprocessResult};
use repo::SubprocessRunner;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::Instant;

/// Environment variable name stripped from subprocess environments by default.
///
/// Prevents nested Claude Code sessions from blocking when launched as
/// subprocesses.
const STRIPPED_ENV_VAR: &str = "CLAUDECODE";

/// Formats a [`Duration`] as an ISO 8601 duration string.
///
/// Matches the Java `Duration.toString()` format: `PT{seconds}S` with
/// fractional seconds when needed. Examples: `PT0.005S`, `PT1.5S`, `PT30S`.
fn format_duration_iso(d: Duration) -> String {
    let secs = d.as_secs();
    let nanos = d.subsec_nanos();

    if nanos == 0 {
        return format!("PT{secs}S");
    }

    // Format fractional seconds, trimming trailing zeros.
    // subsec_nanos is always < 1_000_000_000, so the formatted string is
    // at most 9 digits.
    let frac = format!("{nanos:09}");
    let trimmed = frac.trim_end_matches('0');

    format!("PT{secs}.{trimmed}S")
}

/// Strips a single trailing newline (`\n` or `\r\n`) from the end of a string.
///
/// Matches the Java behavior where trailing newlines are removed from
/// subprocess stdout.
fn strip_trailing_newline(s: &str) -> &str {
    s.strip_suffix("\r\n")
        .or_else(|| s.strip_suffix('\n'))
        .unwrap_or(s)
}

/// RAII guard that kills a child process on drop unless disarmed.
///
/// When the guard is dropped (including on future cancellation), it sends
/// SIGKILL to the child process via [`Child::start_kill`]. Call
/// [`disarm`](Self::disarm) after successful completion to prevent the kill.
struct ChildKillOnDrop {
    child: Option<tokio::process::Child>,
}

impl ChildKillOnDrop {
    /// Disarms the guard, returning the child without killing it.
    ///
    /// After calling this method, the guard will not kill the child on drop.
    const fn disarm(&mut self) -> Option<tokio::process::Child> {
        self.child.take()
    }
}

impl Drop for ChildKillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill(); // reason: best-effort kill in drop, errors cannot be propagated
        }
    }
}

/// Async subprocess runner backed by `tokio::process::Command`.
///
/// Implements the [`SubprocessRunner`] trait from the `repo` crate. Each call
/// to [`run`](SubprocessRunner::run) spawns a new child process, optionally
/// writes stdin data, and collects stdout/stderr concurrently to avoid
/// deadlocks from full pipe buffers.
#[derive(Debug, Clone)]
pub struct TokioSubprocessRunner;

impl TokioSubprocessRunner {
    /// Creates a new `TokioSubprocessRunner`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TokioSubprocessRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Key-value pair for environment variable overlay.
type EnvPair = (String, String);

/// Builds the extra environment variables to overlay on the subprocess.
///
/// Returns a `Vec` of key-value pairs from the optional environment map.
/// The caller is responsible for calling `cmd.env_remove(STRIPPED_ENV_VAR)`
/// separately to strip the blocked env var.
fn collect_extra_env(extra: Option<&EnvironmentMap>) -> Vec<EnvPair> {
    match extra {
        Some(env_map) => env_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        None => Vec::new(),
    }
}

/// Executes the subprocess with an optional timeout.
///
/// This is the core execution logic, extracted to keep the trait impl clean.
async fn execute_subprocess(command: SubprocessCommand) -> Result<SubprocessResult, SchedulrError> {
    let start = Instant::now();

    match command.timeout {
        Some(timeout) => {
            match tokio::time::timeout(timeout, spawn_and_collect(command, start)).await {
                Ok(result) => result,
                Err(_elapsed) => {
                    let elapsed = start.elapsed();
                    let timeout_msg =
                        format!("Process timed out after {}", format_duration_iso(timeout));
                    Ok(SubprocessResult {
                        exit_code: -1,
                        stdout: String::new(),
                        stderr: timeout_msg,
                        elapsed,
                    })
                }
            }
        }
        None => spawn_and_collect(command, start).await,
    }
}

/// Spawns the child process, writes stdin, and collects stdout/stderr.
async fn spawn_and_collect(
    command: SubprocessCommand,
    start: Instant,
) -> Result<SubprocessResult, SchedulrError> {
    let has_stdin = command.stdin_data.is_some();

    let mut cmd = build_command(&command, has_stdin);
    let spawned = cmd.spawn().map_err(SchedulrError::Io)?;
    let mut guard = ChildKillOnDrop {
        child: Some(spawned),
    };

    // Write stdin data if provided, then drop the handle to close the pipe.
    if let Some(stdin_data) = command.stdin_data {
        if let Some(inner) = guard.child.as_mut() {
            if let Some(mut stdin_handle) = inner.stdin.take() {
                stdin_handle
                    .write_all(stdin_data.as_bytes())
                    .await
                    .map_err(SchedulrError::Io)?;
                // Handle is dropped here, closing stdin for the child process.
                drop(stdin_handle);
            }
        }
    }

    // Read stdout and stderr concurrently to prevent deadlock when buffers fill.
    let stdout_handle = guard.child.as_mut().and_then(|c| c.stdout.take());
    let stderr_handle = guard.child.as_mut().and_then(|c| c.stderr.take());

    let (stdout_result, stderr_result) =
        tokio::join!(read_stream(stdout_handle), read_stream(stderr_handle),);

    let stdout_bytes = stdout_result.map_err(SchedulrError::Io)?;
    let stderr_bytes = stderr_result.map_err(SchedulrError::Io)?;

    // Disarm the guard: process completed normally, no need to kill.
    let status = match guard.disarm() {
        Some(mut completed) => completed.wait().await.map_err(SchedulrError::Io)?,
        None => {
            return Err(SchedulrError::Io(std::io::Error::other(
                "child process unexpectedly missing from guard",
            )));
        }
    };
    let elapsed = start.elapsed();

    // On Unix, a process killed by signal has no exit code; default to -1.
    let exit_code = status.code().unwrap_or(-1);

    let stdout_raw = String::from_utf8_lossy(&stdout_bytes);
    let stdout = strip_trailing_newline(&stdout_raw).to_owned();
    let stderr_raw = String::from_utf8_lossy(&stderr_bytes);
    let stderr = strip_trailing_newline(&stderr_raw).to_owned();

    Ok(SubprocessResult {
        exit_code,
        stdout,
        stderr,
        elapsed,
    })
}

/// Builds a `tokio::process::Command` from a [`SubprocessCommand`].
fn build_command(command: &SubprocessCommand, has_stdin: bool) -> Command {
    // The command vec is always ["/bin/sh", "-c", "the command"].
    // First element is the program, rest are arguments.
    let program = command.command.first().map_or("/bin/sh", String::as_str);

    let mut cmd = Command::new(program);

    // Add remaining arguments.
    for arg in command.command.iter().skip(1) {
        let _: &mut Command = cmd.arg(arg);
    }

    // Configure stdio.
    if has_stdin {
        let _: &mut Command = cmd.stdin(Stdio::piped());
    } else {
        let _: &mut Command = cmd.stdin(Stdio::null());
    }
    let _: &mut Command = cmd.stdout(Stdio::piped());
    let _: &mut Command = cmd.stderr(Stdio::piped());

    // Set working directory if provided.
    if let Some(ref dir) = command.working_directory {
        let _: &mut Command = cmd.current_dir(dir);
    }

    // Strip CLAUDECODE env var to prevent blocking nested sessions.
    let _: &mut Command = cmd.env_remove(STRIPPED_ENV_VAR);

    // Overlay extra environment variables.
    let extra_env = collect_extra_env(command.environment.as_ref());
    let _: &mut Command = cmd.envs(extra_env);

    cmd
}

/// Reads all bytes from an optional async stream (stdout or stderr).
async fn read_stream<R: tokio::io::AsyncRead + Unpin>(
    reader: Option<R>,
) -> Result<Vec<u8>, std::io::Error> {
    match reader {
        Some(stream) => {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let mut rdr = tokio::io::BufReader::new(stream);
            let _bytes_read: usize = rdr.read_to_end(&mut buf).await?;
            Ok(buf)
        }
        None => Ok(Vec::new()),
    }
}

impl SubprocessRunner for TokioSubprocessRunner {
    /// Executes a subprocess command and returns the result.
    ///
    /// Spawns a child process using `tokio::process::Command`, optionally
    /// writes stdin data, reads stdout/stderr concurrently, and respects
    /// the configured timeout.
    async fn run(&self, command: SubprocessCommand) -> Result<SubprocessResult, SchedulrError> {
        tracing::debug!(
            cmd = ?command.command,
            working_dir = ?command.working_directory,
            timeout = ?command.timeout,
            has_stdin = command.stdin_data.is_some(),
            "Executing subprocess"
        );

        let result = execute_subprocess(command).await?;

        tracing::debug!(
            exit_code = result.exit_code,
            elapsed = ?result.elapsed,
            stdout_len = result.stdout.len(),
            stderr_len = result.stderr.len(),
            "Subprocess completed"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
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
            command: vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "exit 42".to_owned(),
            ],
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
}
