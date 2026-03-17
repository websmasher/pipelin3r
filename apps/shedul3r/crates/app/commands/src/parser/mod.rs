//! YAML task definition parser.
//!
//! Parses YAML task definitions (from the legacy Java format) into
//! [`TaskDefinition`] values. Handles kebab-case field names, duration
//! parsing (`10s`, `5m`, `1h`, ISO-8601 `PT30S`), and field aliasing
//! (`run` -> `command`).

use std::time::Duration;

use domain_types::{
    BulkheadConfig, CircuitBreakerConfig, RateLimitConfig, RetryConfig, SchedulrError,
    TaskDefinition,
};

/// Shorthand for parser results returning an optional config.
type ParseResult<T> = Result<Option<T>, SchedulrError>;

/// Shorthand for an optional duration parse result.
type DurationResult = Result<Option<Duration>, SchedulrError>;

/// Parse a YAML string into a [`TaskDefinition`].
///
/// The YAML may use `command` or `run` for the shell command, and uses
/// kebab-case field names matching the legacy Java format.
///
/// # Errors
///
/// Returns [`SchedulrError::TaskDefinition`] if the YAML is malformed or
/// missing required fields (`command`/`run`).
pub fn parse_task_definition(yaml: &str) -> Result<TaskDefinition, SchedulrError> {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|_| {
        SchedulrError::TaskDefinition("Failed to parse YAML task definition".to_owned())
    })?;

    let mapping = value.as_mapping().ok_or_else(|| {
        SchedulrError::TaskDefinition("Failed to parse YAML task definition".to_owned())
    })?;

    let command = extract_string(mapping, "command")
        .or_else(|| extract_string(mapping, "run"))
        .ok_or_else(|| {
            SchedulrError::TaskDefinition(
                "Task definition must specify either 'run' or 'command' field".to_owned(),
            )
        })?;

    let name = extract_string(mapping, "name");
    let limiter_key = extract_string(mapping, "provider-id");
    let timeout = extract_duration(mapping, "timeout")?;
    let rate_limit_config = parse_rate_limit(mapping)?;
    let retry_config = parse_retry(mapping)?;
    let circuit_breaker_config = parse_circuit_breaker(mapping)?;
    let bulkhead_config = parse_bulkhead(mapping)?;

    Ok(TaskDefinition {
        name,
        limiter_key,
        command,
        timeout,
        rate_limit_config,
        retry_config,
        circuit_breaker_config,
        bulkhead_config,
    })
}

/// Default rate-limit timeout: 5 minutes.
const DEFAULT_RATE_LIMIT_TIMEOUT_SECS: u64 = 300;

/// Default bulkhead max wait: 5 minutes.
const DEFAULT_BULKHEAD_MAX_WAIT_SECS: u64 = 300;

/// Build a YAML key value from a string.
fn yaml_key(key: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(key.to_owned())
}

/// Extract a string value from a YAML mapping by key.
fn extract_string(mapping: &serde_yaml::Mapping, key: &str) -> Option<String> {
    mapping
        .get(yaml_key(key))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned)
}

/// Extract a `u32` value from a YAML mapping by key.
fn extract_u32(mapping: &serde_yaml::Mapping, key: &str) -> Option<u32> {
    mapping
        .get(yaml_key(key))
        .and_then(serde_yaml::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
}

/// Extract a `f64` value from a YAML mapping by key.
fn extract_f64(mapping: &serde_yaml::Mapping, key: &str) -> Option<f64> {
    let val = mapping.get(yaml_key(key))?;
    // Try as native f64 first, then fall back to u64 converted via string
    val.as_f64().or_else(|| {
        let integer = val.as_u64()?;
        // Convert via string to avoid `as` cast lint
        integer.to_string().parse::<f64>().ok()
    })
}

/// Extract a duration string from a YAML mapping and parse it.
fn extract_duration(mapping: &serde_yaml::Mapping, key: &str) -> DurationResult {
    match extract_string(mapping, key) {
        Some(s) => parse_duration(&s).map(Some),
        None => Ok(None),
    }
}

/// Parse a duration string in human-friendly (`10s`, `5m`, `1h`) or
/// ISO-8601 (`PT1.5S`, `PT30S`) format.
///
/// # Errors
///
/// Returns [`SchedulrError::TaskDefinition`] if the format is unrecognized.
fn parse_duration(input: &str) -> Result<Duration, SchedulrError> {
    let trimmed = input.trim();

    if trimmed.starts_with("PT") || trimmed.starts_with("pt") {
        parse_iso8601_duration(trimmed)
    } else {
        parse_human_duration(trimmed)
    }
}

/// Parse ISO-8601 duration like `PT1.5S`, `PT30S`.
fn parse_iso8601_duration(input: &str) -> Result<Duration, SchedulrError> {
    let body = input.get(2..).ok_or_else(|| {
        SchedulrError::TaskDefinition(format!("Invalid ISO-8601 duration: {input}"))
    })?;

    if !body.ends_with('S') && !body.ends_with('s') {
        return Err(SchedulrError::TaskDefinition(format!(
            "Invalid ISO-8601 duration (only seconds supported): {input}"
        )));
    }

    let end_idx = body.len().checked_sub(1).ok_or_else(|| {
        SchedulrError::TaskDefinition(format!("Invalid ISO-8601 duration: {input}"))
    })?;

    let num_str = body.get(..end_idx).ok_or_else(|| {
        SchedulrError::TaskDefinition(format!("Invalid ISO-8601 duration: {input}"))
    })?;

    let secs: f64 = num_str.parse().map_err(|_| {
        SchedulrError::TaskDefinition(format!("Invalid ISO-8601 duration number: {input}"))
    })?;

    Ok(Duration::from_secs_f64(secs))
}

/// Parse human-friendly duration like `10s`, `5m`, `1h`.
fn parse_human_duration(input: &str) -> Result<Duration, SchedulrError> {
    if input.is_empty() {
        return Err(SchedulrError::TaskDefinition(
            "Empty duration string".to_owned(),
        ));
    }

    let suffix_start = input
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .ok_or_else(|| {
            SchedulrError::TaskDefinition(format!("Duration missing unit suffix: {input}"))
        })?;

    let num_str = input
        .get(..suffix_start)
        .ok_or_else(|| SchedulrError::TaskDefinition(format!("Invalid duration: {input}")))?;

    let suffix = input
        .get(suffix_start..)
        .ok_or_else(|| SchedulrError::TaskDefinition(format!("Invalid duration: {input}")))?;

    let value: f64 = num_str
        .parse()
        .map_err(|_| SchedulrError::TaskDefinition(format!("Invalid duration number: {input}")))?;

    #[allow(clippy::arithmetic_side_effects)] // f64 multiply for small duration values
    let secs = match suffix {
        "s" => value,
        "m" => value * 60.0,
        "h" => value * 3600.0,
        _ => {
            return Err(SchedulrError::TaskDefinition(format!(
                "Unsupported duration suffix '{suffix}' in: {input}"
            )));
        }
    };

    Ok(Duration::from_secs_f64(secs))
}

/// Parse the `rate-limit` sub-mapping into [`RateLimitConfig`].
fn parse_rate_limit(mapping: &serde_yaml::Mapping) -> ParseResult<RateLimitConfig> {
    let rl_value = mapping.get(yaml_key("rate-limit"));
    let Some(rl_mapping) = rl_value.and_then(serde_yaml::Value::as_mapping) else {
        return Ok(None);
    };

    let limit_for_period = extract_u32(rl_mapping, "max-rate").ok_or_else(|| {
        SchedulrError::TaskDefinition(
            "rate-limit.max-rate is required when rate-limit is specified".to_owned(),
        )
    })?;

    let limit_refresh_period = extract_duration(rl_mapping, "window")?.ok_or_else(|| {
        SchedulrError::TaskDefinition(
            "rate-limit.window is required when rate-limit is specified".to_owned(),
        )
    })?;

    Ok(Some(RateLimitConfig {
        limit_for_period,
        limit_refresh_period,
        timeout_duration: Duration::from_secs(DEFAULT_RATE_LIMIT_TIMEOUT_SECS),
    }))
}

/// Parse the `retry` sub-mapping into [`RetryConfig`].
fn parse_retry(mapping: &serde_yaml::Mapping) -> ParseResult<RetryConfig> {
    let retry_value = mapping.get(yaml_key("retry"));
    let Some(retry_mapping) = retry_value.and_then(serde_yaml::Value::as_mapping) else {
        return Ok(None);
    };

    let max_attempts = extract_u32(retry_mapping, "max-retries").ok_or_else(|| {
        SchedulrError::TaskDefinition(
            "retry.max-retries is required when retry is specified".to_owned(),
        )
    })?;

    let wait_duration = extract_duration(retry_mapping, "initial-delay")?.ok_or_else(|| {
        SchedulrError::TaskDefinition(
            "retry.initial-delay is required when retry is specified".to_owned(),
        )
    })?;

    let backoff_multiplier = extract_f64(retry_mapping, "backoff-multiplier").unwrap_or(1.0);

    let max_delay = extract_duration(retry_mapping, "max-delay")?.unwrap_or(wait_duration);

    Ok(Some(RetryConfig {
        max_attempts,
        wait_duration,
        backoff_multiplier,
        max_delay,
    }))
}

/// Parse the `circuit-breaker` sub-mapping into [`CircuitBreakerConfig`].
fn parse_circuit_breaker(mapping: &serde_yaml::Mapping) -> ParseResult<CircuitBreakerConfig> {
    let cb_value = mapping.get(yaml_key("circuit-breaker"));
    let Some(cb_mapping) = cb_value.and_then(serde_yaml::Value::as_mapping) else {
        return Ok(None);
    };

    let failure_rate_threshold =
        extract_f64(cb_mapping, "failure-rate-threshold").ok_or_else(|| {
            SchedulrError::TaskDefinition(
                "circuit-breaker.failure-rate-threshold is required when circuit-breaker is specified"
                    .to_owned(),
            )
        })?;

    let sliding_window_size =
        extract_u32(cb_mapping, "sliding-window-size").ok_or_else(|| {
            SchedulrError::TaskDefinition(
                "circuit-breaker.sliding-window-size is required when circuit-breaker is specified"
                    .to_owned(),
            )
        })?;

    let wait_duration_in_open_state =
        extract_duration(cb_mapping, "wait-duration-in-open-state")?.ok_or_else(|| {
            SchedulrError::TaskDefinition(
                "circuit-breaker.wait-duration-in-open-state is required when circuit-breaker is specified"
                    .to_owned(),
            )
        })?;

    Ok(Some(CircuitBreakerConfig {
        failure_rate_threshold,
        sliding_window_size,
        wait_duration_in_open_state,
    }))
}

/// Parse the `max-concurrent` and `max-wait` fields into [`BulkheadConfig`].
fn parse_bulkhead(mapping: &serde_yaml::Mapping) -> ParseResult<BulkheadConfig> {
    let Some(max_concurrent) = extract_u32(mapping, "max-concurrent") else {
        return Ok(None);
    };

    let max_wait_duration = extract_duration(mapping, "max-wait")?
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_BULKHEAD_MAX_WAIT_SECS));

    Ok(Some(BulkheadConfig {
        max_concurrent,
        max_wait_duration,
    }))
}

#[cfg(test)]
mod tests;
