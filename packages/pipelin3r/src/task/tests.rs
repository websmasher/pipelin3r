#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

#[test]
fn golden_fixture_matches() {
    let config = TaskConfig {
        name: String::from("3_1_implement_tests"),
        model: None,
        timeout: None,
        provider_id: None,
        max_concurrent: None,
        max_wait: None,
        max_retries: None,
        allowed_tools: Some(String::from("Read,Write")),
        retry_initial_delay: None,
        retry_backoff_multiplier: None,
        retry_max_delay: None,
        command_override: None,
    };

    let expected = "\
name: 3_1_implement_tests
command: claude -p --model opus --setting-sources \"\" --permission-mode bypassPermissions --allowedTools Read,Write
timeout: 15m
provider-id: claude
max-concurrent: 3
max-wait: 2h
retry:
  max-retries: 2
  initial-delay: 5s
  backoff-multiplier: 2
  max-delay: 30s
";

    let result = build_task_yaml(&config).unwrap();
    assert_eq!(result, expected, "Task YAML does not match golden fixture");
}

#[test]
fn custom_values_override_defaults() {
    let config = TaskConfig {
        name: String::from("my-task"),
        model: Some(String::from("sonnet")),
        timeout: Some(String::from("30m")),
        provider_id: Some(String::from("openai")),
        max_concurrent: Some(5),
        max_wait: Some(String::from("1h")),
        max_retries: Some(0),
        allowed_tools: None,
        retry_initial_delay: Some(String::from("10s")),
        retry_backoff_multiplier: Some(3.0),
        retry_max_delay: Some(String::from("1m")),
        command_override: None,
    };

    let result = build_task_yaml(&config).unwrap();
    assert!(result.contains("--model sonnet"), "should use custom model");
    assert!(result.contains("timeout: 30m"), "should use custom timeout");
    assert!(
        result.contains("provider-id: openai"),
        "should use custom provider"
    );
    assert!(
        result.contains("max-concurrent: 5"),
        "should use custom concurrency"
    );
    assert!(
        !result.contains("--allowedTools"),
        "should omit tools when None"
    );
    assert!(
        result.contains("initial-delay: 10s"),
        "should use custom initial delay"
    );
    assert!(
        result.contains("backoff-multiplier: 3"),
        "should use custom backoff multiplier"
    );
    assert!(
        result.contains("max-delay: 1m"),
        "should use custom max delay"
    );
}
