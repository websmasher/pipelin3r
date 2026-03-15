//! Task YAML builder for shedul3r task definitions.

use std::fmt::Write;

/// Configuration for generating a task YAML definition.
pub struct TaskConfig {
    pub name: String,
    pub model: Option<String>,
    pub timeout: Option<String>,
    pub provider_id: Option<String>,
    pub max_concurrent: Option<usize>,
    pub max_wait: Option<String>,
    pub max_retries: Option<usize>,
    pub allowed_tools: Option<String>,
}

/// Build a task YAML string from the given configuration, applying defaults.
///
/// # Errors
/// Returns an error if string formatting fails (should not happen in practice).
pub fn build_task_yaml(config: &TaskConfig) -> anyhow::Result<String> {
    let model = config.model.as_deref().unwrap_or("opus");
    let timeout = config.timeout.as_deref().unwrap_or("15m");
    let provider_id = config.provider_id.as_deref().unwrap_or("claude");
    let max_concurrent = config.max_concurrent.unwrap_or(3);
    let max_wait = config.max_wait.as_deref().unwrap_or("2h");
    let max_retries = config.max_retries.unwrap_or(2);

    let mut command = format!(
        "claude -p --model {model} --setting-sources \"\" --permission-mode bypassPermissions"
    );
    if let Some(tools) = &config.allowed_tools {
        write!(&mut command, " --allowedTools {tools}")?;
    }

    let mut out = String::new();
    writeln!(&mut out, "name: {}", config.name)?;
    writeln!(&mut out, "command: {command}")?;
    writeln!(&mut out, "timeout: {timeout}")?;
    writeln!(&mut out, "provider-id: {provider_id}")?;
    writeln!(&mut out, "max-concurrent: {max_concurrent}")?;
    writeln!(&mut out, "max-wait: {max_wait}")?;
    writeln!(&mut out, "retry:")?;
    writeln!(&mut out, "  max-retries: {max_retries}")?;
    writeln!(&mut out, "  initial-delay: 5s")?;
    writeln!(&mut out, "  backoff-multiplier: 2.0")?;
    writeln!(&mut out, "  max-delay: 30s")?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion
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
    #[allow(clippy::unwrap_used)] // reason: test assertion
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
        assert!(
            result.contains("--model sonnet"),
            "should use custom model"
        );
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
}
