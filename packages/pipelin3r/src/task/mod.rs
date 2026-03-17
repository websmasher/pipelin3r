//! Task YAML builder for shedul3r task definitions.

use std::fmt::Write;

use crate::error::PipelineError;

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
pub fn build_task_yaml(config: &TaskConfig) -> Result<String, PipelineError> {
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
        write!(&mut command, " --allowedTools {tools}")
            .map_err(|e| PipelineError::Config(format!("failed to format command: {e}")))?;
    }

    let mut out = String::new();
    writeln!(&mut out, "name: {}", config.name)
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "command: {command}")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
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
    writeln!(&mut out, "  initial-delay: 5s")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "  backoff-multiplier: 2.0")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;
    writeln!(&mut out, "  max-delay: 30s")
        .map_err(|e| PipelineError::Config(format!("failed to format task YAML: {e}")))?;

    Ok(out)
}

#[cfg(test)]
mod tests;
