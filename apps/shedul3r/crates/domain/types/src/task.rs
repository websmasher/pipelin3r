//! Task definition type parsed from YAML input.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::{BulkheadConfig, CircuitBreakerConfig, RateLimitConfig, RetryConfig};
use crate::duration_serde;

/// A task definition describing a shell command and its resilience settings.
///
/// Parsed from YAML provided by the caller. The `limiter_key` groups tasks
/// that share rate limits, circuit breakers, and bulkheads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Optional human-readable name for logging and status display.
    pub name: Option<String>,
    /// Key used to group tasks under the same rate limiter / circuit breaker.
    /// Corresponds to `provider-id` in the YAML input.
    pub limiter_key: Option<String>,
    /// Shell command to execute.
    pub command: String,
    /// Maximum execution time for the command.
    #[serde(
        default,
        with = "duration_serde::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout: Option<Duration>,
    /// Rate limiting configuration for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_config: Option<RateLimitConfig>,
    /// Retry configuration for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_config: Option<RetryConfig>,
    /// Circuit breaker configuration for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    /// Bulkhead (concurrency limiting) configuration for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bulkhead_config: Option<BulkheadConfig>,
}
