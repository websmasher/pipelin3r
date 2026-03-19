//! Task YAML builder for shedul3r task definitions.

use std::fmt::Write;

use crate::error::PipelineError;

/// Configuration for generating a task YAML definition.
pub struct TaskConfig {
    /// Task name.
    pub name: String,
    /// Model identifier for the `--model` flag.
    pub model: Option<String>,
    /// Timeout string (e.g., `"15m"`).
    pub timeout: Option<String>,
    /// Provider/limiter key (e.g., `"claude"`).
    pub provider_id: Option<String>,
    /// Maximum concurrent tasks for this provider key.
    pub max_concurrent: Option<usize>,
    /// Maximum queue wait time string (e.g., `"2h"`).
    pub max_wait: Option<String>,
    /// Maximum retry attempts.
    pub max_retries: Option<usize>,
    /// Comma-separated allowed tools string.
    pub allowed_tools: Option<String>,
    /// Initial retry delay string (e.g., `"5s"`).
    pub retry_initial_delay: Option<String>,
    /// Retry backoff multiplier.
    pub retry_backoff_multiplier: Option<f64>,
    /// Maximum retry delay string (e.g., `"30s"`).
    pub retry_max_delay: Option<String>,
    /// Raw command override. If set, used instead of building a Claude
    /// Code invocation. This allows running arbitrary shell commands
    /// through shedul3r without an LLM.
    pub command_override: Option<String>,
}

/// Build a task YAML string from the given configuration, applying defaults.
///
/// # Errors
/// Returns an error if string formatting fails (should not happen in practice).
pub fn build_task_yaml(config: &TaskConfig) -> Result<String, PipelineError> {
    let model = config.model.as_deref().unwrap_or("opus");
    let timeout = config.timeout.as_deref().unwrap_or("15m");
    let provider_id = config.provider_id.as_deref().unwrap_or("claude");
    let max_concurrent = config.max_concurrent.unwrap_or(3);
    let max_wait = config.max_wait.as_deref().unwrap_or("2h");
    let max_retries = config.max_retries.unwrap_or(2);
    let initial_delay = config.retry_initial_delay.as_deref().unwrap_or("5s");
    let backoff_multiplier = config.retry_backoff_multiplier.unwrap_or(2.0);
    let max_delay = config.retry_max_delay.as_deref().unwrap_or("30s");

    let command = if let Some(ref cmd) = config.command_override {
        cmd.clone()
    } else {
        let mut cmd = format!(
            "claude -p --model {model} --setting-sources \"\" --permission-mode bypassPermissions"
        );
        if let Some(tools) = &config.allowed_tools {
            write!(&mut cmd, " --allowedTools {tools}")
                .map_err(|e| PipelineError::Config(format!("failed to format command: {e}")))?;
        }
        cmd
    };

    let mut out = String::new();
    writeln!(&mut out, "name: {}", config.name)
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    // Multi-line commands need YAML literal block scalar (|).
    if command.contains('\n') {
        writeln!(&mut out, "command: |")
            .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
        for line in command.lines() {
            writeln!(&mut out, "  {line}")
                .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
        }
    } else {
        writeln!(&mut out, "command: {command}")
            .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    }
    writeln!(&mut out, "timeout: {timeout}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "provider-id: {provider_id}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "max-concurrent: {max_concurrent}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "max-wait: {max_wait}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "retry:")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "  max-retries: {max_retries}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "  initial-delay: {initial_delay}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "  backoff-multiplier: {backoff_multiplier}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "  max-delay: {max_delay}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;

    Ok(out)
}

#[cfg(test)]
mod tests;
