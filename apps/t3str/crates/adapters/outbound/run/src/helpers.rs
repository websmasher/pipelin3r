//! Shared helpers for language executor modules.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestResult, TestStatus, TestSummary};

/// Build a [`TestSummary`] from a slice of test results.
pub fn build_summary(results: &[TestResult]) -> TestSummary {
    let mut summary = TestSummary::default();
    for result in results {
        summary.total = summary.total.saturating_add(1);
        match result.status {
            TestStatus::Passed => summary.passed = summary.passed.saturating_add(1),
            TestStatus::Failed => summary.failed = summary.failed.saturating_add(1),
            TestStatus::Skipped => summary.skipped = summary.skipped.saturating_add(1),
            TestStatus::Error => summary.errors = summary.errors.saturating_add(1),
        }
    }
    summary
}

/// Output from running a command: `(stdout, stderr, exit_code)`.
pub type CommandOutput = (String, String, i32);

/// A single environment variable key-value pair for command execution.
pub type EnvVar<'a> = (&'a str, &'a str);

/// Run an external command with timeout, capturing stdout and stderr.
///
/// Spawns the given `program` with `args` in `work_dir`, applying any extra
/// environment variables from `env_vars`. If the command does not finish
/// within `timeout_secs`, returns [`T3strError::ExecutionFailed`].
///
/// # Errors
///
/// Returns [`T3strError::ExecutionFailed`] on timeout or
/// [`T3strError::Io`] if the process cannot be spawned.
pub async fn run_command(
    program: &str,
    args: &[&str],
    work_dir: &Path,
    env_vars: &[EnvVar<'_>],
    timeout_secs: u64,
    language: Language,
) -> Result<CommandOutput, T3strError> {
    use tokio::process::Command;

    let mut cmd = Command::new(program);
    let _ = cmd
        .args(args)
        .current_dir(work_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    for (key, val) in env_vars {
        let _ = cmd.env(key, val);
    }

    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
        .await
        .map_err(|_| T3strError::ExecutionFailed {
            language,
            reason: format!("test execution timed out after {timeout_secs}s"),
        })?
        .map_err(T3strError::Io)?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, code))
}

/// Truncate a string to at most `max_chars` characters from the end.
///
/// Used for `raw_output` to cap memory usage while preserving the most
/// recent (and usually most relevant) output.
pub fn truncate_output(output: &str, max_chars: usize) -> String {
    if output.len() <= max_chars {
        return output.to_owned();
    }
    let start = output.len().saturating_sub(max_chars);
    // Walk forward to find a valid char boundary.
    let mut boundary = start;
    while !output.is_char_boundary(boundary) && boundary < output.len() {
        boundary = boundary.saturating_add(1);
    }
    output.get(boundary..).unwrap_or_default().to_owned()
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;
