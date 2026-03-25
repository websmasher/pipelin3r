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
    /// Declared output files that determine success for file-writing agent
    /// tasks. When present, the generated Claude command exits successfully
    /// once these outputs become non-empty and stable, even if Claude itself
    /// keeps the session open.
    pub success_on_outputs: Option<Vec<String>>,
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
        let mut claude_cmd = format!(
            "claude -p --model {model} --setting-sources \"\" --permission-mode bypassPermissions"
        );
        if let Some(tools) = &config.allowed_tools {
            write!(&mut claude_cmd, " --allowedTools {tools}")
                .map_err(|e| PipelineError::Config(format!("failed to format command: {e}")))?;
        }
        if let Some(outputs) = config.success_on_outputs.as_ref().filter(|v| !v.is_empty()) {
            build_file_watcher_command(&claude_cmd, outputs)
        } else {
            claude_cmd
        }
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

/// Build a shell wrapper that treats non-empty stable output files as success.
fn build_file_watcher_command(claude_cmd: &str, outputs: &[String]) -> String {
    let outputs_array = outputs
        .iter()
        .map(|path| format!("'{}'", shell_single_quote(path)))
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        "set +e\n\
         prompt_file=$(mktemp)\n\
         cat > \"$prompt_file\"\n\
         {claude_cmd} < \"$prompt_file\" &\n\
         claude_pid=$!\n\
         last_sig=''\n\
         stable_hits=0\n\
         while kill -0 \"$claude_pid\" 2>/dev/null; do\n\
           ready=1\n\
           sig=''\n\
           for path in {outputs_array}; do\n\
             if [ ! -f \"$path\" ]; then\n\
               ready=0\n\
               break\n\
             fi\n\
             size=$(wc -c < \"$path\" 2>/dev/null || echo 0)\n\
             sig=\"$sig|$path:$size\"\n\
             if [ \"$size\" -le 0 ]; then\n\
               ready=0\n\
             fi\n\
           done\n\
           if [ \"$ready\" -eq 1 ]; then\n\
             if [ \"$sig\" = \"$last_sig\" ]; then\n\
               stable_hits=$((stable_hits + 1))\n\
             else\n\
               stable_hits=0\n\
               last_sig=\"$sig\"\n\
             fi\n\
             if [ \"$stable_hits\" -ge 2 ]; then\n\
               kill \"$claude_pid\" 2>/dev/null || true\n\
               wait \"$claude_pid\" 2>/dev/null || true\n\
               exit 0\n\
             fi\n\
           fi\n\
           sleep 2\n\
         done\n\
         wait \"$claude_pid\"\n\
         exit_code=$?\n\
         rm -f \"$prompt_file\"\n\
         ready=1\n\
         for path in {outputs_array}; do\n\
           if [ ! -f \"$path\" ]; then\n\
             ready=0\n\
             break\n\
           fi\n\
           size=$(wc -c < \"$path\" 2>/dev/null || echo 0)\n\
           if [ \"$size\" -le 0 ]; then\n\
             ready=0\n\
             break\n\
           fi\n\
         done\n\
         if [ \"$ready\" -eq 1 ]; then\n\
           exit 0\n\
         fi\n\
         exit \"$exit_code\""
    )
}

/// Escape a string for safe inclusion inside single-quoted shell literals.
fn shell_single_quote(input: &str) -> String {
    input.replace('\'', "'\"'\"'")
}

#[cfg(test)]
mod tests;
