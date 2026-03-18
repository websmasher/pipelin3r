//! Task execution engine — orchestrates subprocess execution with resilience.
//!
//! [`TaskEngine`] is the core application service. It receives port trait
//! implementations via generics and coordinates YAML parsing, rate limiting,
//! circuit breaking, bulkhead concurrency control, retry, and subprocess
//! execution.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};

use domain_types::{
    CircuitBreakerConfig, ExecutionMetadata, LimiterKeyStatus, SchedulerStatus, SchedulrError,
    SubprocessCommand, SubprocessResult, TaskRequest, TaskResponse,
};
use repo::{Bulkhead, CircuitBreaker, RateLimiter, RetryExecutor, SubprocessRunner};

use crate::parser::parse_task_definition;

/// Default circuit breaker: 50% failure rate threshold.
const DEFAULT_CB_FAILURE_RATE: f64 = 50.0;

/// Default circuit breaker: 10-call sliding window.
const DEFAULT_CB_WINDOW_SIZE: u32 = 10;

/// Default circuit breaker: 30-second open-state wait.
const DEFAULT_CB_WAIT_SECS: u64 = 30;

/// RAII guard that releases a bulkhead permit on drop.
///
/// Ensures permits are always released even if the future is cancelled
/// (e.g. client disconnect, timeout) between acquire and release.
struct BulkheadPermitGuard<B: Bulkhead> {
    bulkhead: Arc<B>,
    key: String,
}

impl<B: Bulkhead> Drop for BulkheadPermitGuard<B> {
    fn drop(&mut self) {
        self.bulkhead.release(&self.key);
    }
}

/// Task execution engine that orchestrates subprocess execution with
/// rate limiting, circuit breaking, bulkhead concurrency control, and retry.
///
/// Generic over all port traits so concrete adapters are injected at
/// construction time (in the API composition root).
pub struct TaskEngine<S, R, C, B, Re> {
    subprocess: Arc<S>,
    rate_limiter: Arc<R>,
    circuit_breaker: Arc<C>,
    bulkhead: Arc<B>,
    retry_executor: Arc<Re>,
    active_tasks: AtomicU32,
    started_at: String,
}

impl<S, R, C, B, Re> std::fmt::Debug for TaskEngine<S, R, C, B, Re> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskEngine")
            .field("active_tasks", &self.active_tasks)
            .field("started_at", &self.started_at)
            .finish_non_exhaustive()
    }
}

impl<S, R, C, B, Re> TaskEngine<S, R, C, B, Re>
where
    S: SubprocessRunner + 'static,
    R: RateLimiter,
    C: CircuitBreaker + 'static,
    B: Bulkhead,
    Re: RetryExecutor,
{
    /// Create a new task engine with the given port adapters.
    pub fn new(
        subprocess: Arc<S>,
        rate_limiter: Arc<R>,
        circuit_breaker: Arc<C>,
        bulkhead: Arc<B>,
        retry_executor: Arc<Re>,
    ) -> Self {
        Self {
            subprocess,
            rate_limiter,
            circuit_breaker,
            bulkhead,
            retry_executor,
            active_tasks: AtomicU32::new(0),
            started_at: format_system_time_iso8601(SystemTime::now()),
        }
    }

    /// Execute a task from a [`TaskRequest`].
    ///
    /// All execution outcomes — including subprocess failures, retry exhaustion,
    /// rate limit rejections — are encoded in `Ok(TaskResponse)`.
    ///
    /// # Errors
    ///
    /// Returns `Err(SchedulrError::TaskDefinition)` if the YAML cannot be parsed.
    pub async fn execute(&self, request: TaskRequest) -> Result<TaskResponse, SchedulrError> {
        let started_at = format_system_time_iso8601(SystemTime::now());
        let start = Instant::now();

        // Parse YAML — propagate parse errors to caller
        let definition = parse_task_definition(&request.task)?;

        let limiter_key = request
            .limiter_key
            .or_else(|| definition.limiter_key.clone());

        let timeout = request
            .timeout_ms
            .map(Duration::from_millis)
            .or(definition.timeout);

        let _ = self.active_tasks.fetch_add(1, Ordering::Relaxed);

        // Acquire resilience permits (rate limit, circuit breaker, bulkhead).
        // The returned guard (if any) releases the bulkhead permit on drop.
        // Use per-task circuit breaker config if provided, otherwise fall back to defaults.
        let cb_config = definition
            .circuit_breaker_config
            .clone()
            .unwrap_or_else(default_circuit_breaker_config);

        let mut permit_guard: Option<BulkheadPermitGuard<B>> = None;
        if let Some(ref key) = limiter_key {
            match self
                .acquire_resilience_permits(key, &definition, &cb_config, &started_at, start)
                .await
            {
                Ok(guard) => permit_guard = guard,
                Err(early) => {
                    let _ = self.active_tasks.fetch_sub(1, Ordering::Relaxed);
                    return Ok(early);
                }
            }
        }

        // Validate working directory if provided.
        if let Some(ref dir) = request.working_directory {
            validate_working_directory(dir)?;
        }

        // Build subprocess command
        let cmd = SubprocessCommand {
            command: vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                definition.command.clone(),
            ],
            working_directory: request.working_directory,
            environment: request.environment,
            timeout,
            stdin_data: request.input,
        };

        // Execute (with or without retry)
        let exec_result = if let Some(ref retry_config) = definition.retry_config {
            self.execute_with_retry(cmd, retry_config).await
        } else {
            self.subprocess.run(cmd).await
        };

        let elapsed = start.elapsed();
        let _ = self.active_tasks.fetch_sub(1, Ordering::Relaxed);

        // Drop the permit guard explicitly before building the response.
        // This is not strictly necessary (it would drop at end of scope),
        // but makes the release point visible.
        drop(permit_guard);

        // Record circuit breaker outcome
        if let Some(ref key) = limiter_key {
            match &exec_result {
                Ok(r) if r.exit_code == 0 => self.circuit_breaker.record_success(key),
                Ok(_) | Err(_) => self.circuit_breaker.record_failure(key),
            }
        }

        Ok(exec_result_to_response(exec_result, started_at, elapsed))
    }

    /// Return the current scheduler status.
    pub fn status(&self) -> SchedulerStatus {
        SchedulerStatus {
            active_tasks: self.active_tasks.load(Ordering::Relaxed),
            pending_tasks: 0,
            started_at: self.started_at.clone(),
        }
    }

    /// Return the status of all known limiter keys.
    ///
    /// **Intentionally deferred:** Returns an empty list because the current
    /// hexagonal architecture passes adapters as trait-bounded generics.
    /// Inspecting adapter state (e.g. rate limiter permit counts, circuit
    /// breaker open/closed) requires adding state-inspection methods to the
    /// port traits (`RateLimiter`, `CircuitBreaker`). This is deferred until
    /// the port trait interfaces are extended to support introspection.
    pub const fn limiter_status(&self) -> Vec<LimiterKeyStatus> {
        Vec::new()
    }

    /// Execute a subprocess with retry, treating non-zero exit codes as retryable failures.
    async fn execute_with_retry(
        &self,
        cmd: SubprocessCommand,
        retry_config: &domain_types::RetryConfig,
    ) -> Result<SubprocessResult, SchedulrError> {
        let subprocess = Arc::clone(&self.subprocess);

        self.retry_executor
            .execute_with_retry(
                || {
                    let sub = Arc::clone(&subprocess);
                    let c = cmd.clone();
                    async move {
                        let result = sub.run(c).await?;
                        if result.exit_code != 0 {
                            Err(SchedulrError::Subprocess {
                                exit_code: result.exit_code,
                                message: result.stderr,
                            })
                        } else {
                            Ok(result)
                        }
                    }
                },
                retry_config,
            )
            .await
    }

    /// Acquire rate limit, circuit breaker, and bulkhead permits.
    ///
    /// Returns `Ok(Some(guard))` if a bulkhead permit was acquired (the guard
    /// releases it on drop), `Ok(None)` if no bulkhead was configured, or
    /// `Err(failure_response)` if any permit acquisition failed.
    async fn acquire_resilience_permits(
        &self,
        key: &str,
        definition: &domain_types::TaskDefinition,
        cb_config: &CircuitBreakerConfig,
        started_at: &str,
        start: Instant,
    ) -> Result<Option<BulkheadPermitGuard<B>>, TaskResponse> {
        // Rate limit check
        if let Some(ref rl_config) = definition.rate_limit_config {
            if let Err(e) = self.rate_limiter.acquire_permission(key, rl_config).await {
                return Err(build_failure_response(
                    started_at.to_owned(),
                    start,
                    &e.to_string(),
                ));
            }
        }

        // Circuit breaker check
        if let Err(e) = self.circuit_breaker.check_permitted(key, cb_config) {
            return Err(build_failure_response(
                started_at.to_owned(),
                start,
                &e.to_string(),
            ));
        }

        // Bulkhead acquire
        if let Some(ref bh_config) = definition.bulkhead_config {
            if let Err(e) = self.bulkhead.acquire(key, bh_config).await {
                return Err(build_failure_response(
                    started_at.to_owned(),
                    start,
                    &e.to_string(),
                ));
            }
            return Ok(Some(BulkheadPermitGuard {
                bulkhead: Arc::clone(&self.bulkhead),
                key: key.to_owned(),
            }));
        }

        Ok(None)
    }
}

/// Maximum bytes of subprocess stdout included in the response.
///
/// Claude Code sessions can produce megabytes of conversation log on stdout.
/// Including all of that in the JSON response causes OOM under load and
/// HTTP decode failures on the client. The response carries only the tail
/// of stdout — enough for debugging — while the real work product lives
/// in files the agent wrote to the work directory.
const MAX_OUTPUT_BYTES: usize = 32_768; // 32 KB

/// Truncate a string to at most `max_bytes`, keeping the tail.
///
/// If truncation occurs, prepends `[truncated {original_len} bytes, showing last {max_bytes}]\n`.
/// Always cuts on a char boundary.
fn truncate_output(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return String::from(s);
    }

    let start = s.len().saturating_sub(max_bytes);
    // Walk forward to the next char boundary.
    let mut boundary = start;
    while boundary < s.len() && !s.is_char_boundary(boundary) {
        boundary = boundary.saturating_add(1);
    }

    let tail = s.get(boundary..).unwrap_or("");
    format!(
        "[truncated {} bytes, showing last {}]\n{}",
        s.len(),
        tail.len(),
        tail
    )
}

/// Build a [`TaskResponse`] from a subprocess result.
fn build_task_response(
    result: SubprocessResult,
    started_at: String,
    elapsed: Duration,
) -> TaskResponse {
    if result.exit_code == 0 {
        TaskResponse {
            success: true,
            output: truncate_output(&result.stdout, MAX_OUTPUT_BYTES),
            metadata: ExecutionMetadata {
                started_at,
                elapsed,
                exit_code: 0,
            },
        }
    } else {
        let raw = format!("Exit {}: {}", result.exit_code, result.stderr);
        TaskResponse {
            success: false,
            output: truncate_output(&raw, MAX_OUTPUT_BYTES),
            metadata: ExecutionMetadata {
                started_at,
                elapsed,
                exit_code: result.exit_code,
            },
        }
    }
}

/// Build a failure [`TaskResponse`] with the given error message.
fn build_failure_response(started_at: String, start: Instant, message: &str) -> TaskResponse {
    TaskResponse {
        success: false,
        output: message.to_owned(),
        metadata: ExecutionMetadata {
            started_at,
            elapsed: start.elapsed(),
            exit_code: 1,
        },
    }
}

/// Convert an execution result (success, retry exhaustion, or error) into a [`TaskResponse`].
fn exec_result_to_response(
    result: Result<SubprocessResult, SchedulrError>,
    started_at: String,
    elapsed: Duration,
) -> TaskResponse {
    match result {
        Ok(r) => build_task_response(r, started_at, elapsed),
        Err(SchedulrError::Resilience(limit3r::Limit3rError::RetryExhausted {
            attempts,
            ref last_message,
        })) => TaskResponse {
            success: false,
            output: format!("All {attempts} retry attempts exhausted (last error: {last_message})"),
            metadata: ExecutionMetadata {
                started_at,
                elapsed,
                exit_code: 1,
            },
        },
        Err(other) => TaskResponse {
            success: false,
            output: other.to_string(),
            metadata: ExecutionMetadata {
                started_at,
                elapsed,
                exit_code: 1,
            },
        },
    }
}

/// Build the default circuit breaker configuration used when no per-task
/// `circuit-breaker` YAML block is provided.
const fn default_circuit_breaker_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        failure_rate_threshold: DEFAULT_CB_FAILURE_RATE,
        sliding_window_size: DEFAULT_CB_WINDOW_SIZE,
        wait_duration_in_open_state: Duration::from_secs(DEFAULT_CB_WAIT_SECS),
    }
}

/// Validate that a working directory path is safe for subprocess execution.
///
/// # Rules
/// - Must be an absolute path
/// - Must not contain path traversal components (`..`)
/// - Must exist and be a directory on the filesystem
///
/// # Errors
///
/// Returns [`SchedulrError::TaskDefinition`] if any rule is violated.
fn validate_working_directory(dir: &std::path::Path) -> Result<(), SchedulrError> {
    if !dir.is_absolute() {
        return Err(SchedulrError::TaskDefinition(format!(
            "working_directory must be an absolute path, got: {}",
            dir.display()
        )));
    }

    // Reject path traversal components.
    for component in dir.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(SchedulrError::TaskDefinition(format!(
                "working_directory must not contain '..' path traversal: {}",
                dir.display()
            )));
        }
    }

    if !dir.is_dir() {
        return Err(SchedulrError::TaskDefinition(format!(
            "working_directory does not exist or is not a directory: {}",
            dir.display()
        )));
    }

    Ok(())
}

/// Format a [`SystemTime`] as an ISO-8601 string (UTC, second precision).
///
/// Falls back to the Unix epoch if the system clock is before 1970.
fn format_system_time_iso8601(time: SystemTime) -> String {
    let duration_since_epoch = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);

    format_unix_timestamp(duration_since_epoch.as_secs())
}

/// Format a Unix timestamp (seconds since epoch) as `YYYY-MM-DDTHH:MM:SSZ`.
#[allow(clippy::arithmetic_side_effects)] // all values are bounded by calendar math
fn format_unix_timestamp(total_secs: u64) -> String {
    const SECS_PER_MINUTE: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let days = total_secs / SECS_PER_DAY;
    let remaining_secs = total_secs % SECS_PER_DAY;
    let hours = remaining_secs / SECS_PER_HOUR;
    let remaining_after_hours = remaining_secs % SECS_PER_HOUR;
    let minutes = remaining_after_hours / SECS_PER_MINUTE;
    let seconds = remaining_after_hours % SECS_PER_MINUTE;

    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert a day count since Unix epoch to (year, month, day).
///
/// Uses the civil calendar algorithm adapted from Howard Hinnant's
/// `days_from_civil` (public domain).
#[allow(clippy::arithmetic_side_effects)] // calendar algorithm with bounded values
const fn days_to_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1461 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    (year, month, day)
}

#[cfg(test)]
mod tests;
