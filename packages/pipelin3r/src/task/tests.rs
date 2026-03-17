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
  backoff-multiplier: 2.0
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
}
