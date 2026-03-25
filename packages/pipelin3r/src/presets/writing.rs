//! Writing preset built on top of the verified-step convergence loop.
//!
//! The preset takes a caller-owned working directory plus three user prompts
//! (writer, critic, rewriter), then lowers that into a [`crate::VerifiedStep`]
//! with:
//!
//! - a writer doer that emits `draft.md`
//! - an optional `ProseSmasher` script breaker
//! - a critic agent breaker
//! - a rewriter fixer
//!
//! The caller's working directory is treated as an opaque workspace. The preset
//! discovers its top-level contents and declares those entries as inputs so they
//! are copied into each iteration directory and transported correctly in remote
//! execution mode.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::AgentConfig;
use crate::Breaker;
use crate::Executor;
use crate::PipelineError;
use crate::PromptedStep;
use crate::Var;
use crate::VerifiedStep;
use crate::VerifiedStepResult;
use crate::run_verified_step;

/// Relative filename for the canonical draft within each iteration directory.
const DRAFT_PATH: &str = "draft.md";
/// Relative filename for the critic's report within a breaker directory.
const CRITIC_REPORT_PATH: &str = "critic-report.json";
/// Relative filename for the `ProseSmasher` report within an iteration directory.
const PROSESMASHER_REPORT_PATH: &str = "prosemasher-report.json";
/// Relative path to the critic report from the iteration directory root.
const CRITIC_REPORT_INPUT_PATH: &str = "breaker-critic/critic-report.json";
/// Default step name used when the caller does not override it.
const DEFAULT_STEP_NAME: &str = "writing";
/// Shipped `prosesmasher` preset used by the writing step.
const PROSESMASHER_PRESET: &str = "general-en";

/// Configuration for the writing preset.
#[derive(Debug, Clone)]
pub struct WritingStepConfig {
    /// Step name; determines the subdirectory created under `work_dir`.
    pub name: String,
    /// Root working directory supplied by the caller.
    pub work_dir: PathBuf,
    /// User-owned instruction for the writer doer.
    pub writer_prompt: String,
    /// User-owned instruction for the critic breaker.
    pub critic_prompt: String,
    /// User-owned instruction for the rewriter fixer.
    pub rewriter_prompt: String,
    /// Whether to run `ProseSmasher` as a deterministic script breaker.
    pub use_prosemasher: bool,
    /// Maximum fixer iterations after the initial doer run.
    pub max_iterations: usize,
}

impl WritingStepConfig {
    /// Create a new writing-step configuration with sensible defaults.
    #[must_use]
    pub fn new(
        work_dir: PathBuf,
        writer_prompt: impl Into<String>,
        critic_prompt: impl Into<String>,
        rewriter_prompt: impl Into<String>,
    ) -> Self {
        Self {
            name: String::from(DEFAULT_STEP_NAME),
            work_dir,
            writer_prompt: writer_prompt.into(),
            critic_prompt: critic_prompt.into(),
            rewriter_prompt: rewriter_prompt.into(),
            use_prosemasher: true,
            max_iterations: 3,
        }
    }
}

/// Default critic prompt used by the CLI wrapper when the caller does not
/// supply one explicitly.
pub const DEFAULT_CRITIC_PROMPT: &str = "Review the draft for clarity, structure, unsupported claims, redundancy, \
     factual drift, and failure to satisfy the writer instruction.";

/// Default rewriter prompt used by the CLI wrapper when the caller does not
/// supply one explicitly.
pub const DEFAULT_REWRITER_PROMPT: &str = "Revise the draft to fully address the review findings while preserving \
     what is already strong.";

/// Build a verified step implementing the writing preset.
///
/// # Errors
///
/// Returns an error if the configured working directory does not exist, is not
/// a directory, or its contents cannot be enumerated.
pub fn build_writing_step(
    config: &WritingStepConfig,
    agent_defaults: AgentConfig,
) -> Result<VerifiedStep, PipelineError> {
    validate_workspace_dir(&config.work_dir)?;
    let workspace_inputs = discover_workspace_inputs(&config.work_dir, &config.name)?;

    let mut breakers = Vec::new();
    if config.use_prosemasher {
        breakers.push(build_prosemasher_breaker());
    }
    breakers.push(build_critic_breaker(
        &config.name,
        &config.critic_prompt,
        &workspace_inputs,
    ));

    Ok(VerifiedStep {
        name: config.name.clone(),
        doer: PromptedStep {
            name: format!("{}-writer", config.name),
            prompt_template: writing_prompt_path("writer.md"),
            vars: vec![
                Var::String {
                    placeholder: String::from("{{WRITER_PROMPT}}"),
                    value: config.writer_prompt.clone(),
                },
                Var::String {
                    placeholder: String::from("{{OUTPUT_PATH}}"),
                    value: String::from(DRAFT_PATH),
                },
            ],
            inputs: workspace_inputs.clone(),
            outputs: vec![String::from(DRAFT_PATH)],
        },
        breakers,
        fixer: PromptedStep {
            name: format!("{}-rewriter", config.name),
            prompt_template: writing_prompt_path("rewriter.md"),
            vars: vec![
                Var::String {
                    placeholder: String::from("{{WRITER_PROMPT}}"),
                    value: config.writer_prompt.clone(),
                },
                Var::String {
                    placeholder: String::from("{{REWRITER_PROMPT}}"),
                    value: config.rewriter_prompt.clone(),
                },
                Var::String {
                    placeholder: String::from("{{DRAFT_PATH}}"),
                    value: String::from(DRAFT_PATH),
                },
                Var::String {
                    placeholder: String::from("{{ISSUES_PATH}}"),
                    value: String::from("issues.md"),
                },
                Var::String {
                    placeholder: String::from("{{CRITIC_REPORT_PATH}}"),
                    value: String::from(CRITIC_REPORT_INPUT_PATH),
                },
                Var::String {
                    placeholder: String::from("{{PROSESMASHER_REPORT_PATH}}"),
                    value: String::from(PROSESMASHER_REPORT_PATH),
                },
                Var::String {
                    placeholder: String::from("{{OUTPUT_PATH}}"),
                    value: String::from(DRAFT_PATH),
                },
            ],
            inputs: fixer_inputs(&workspace_inputs, config.use_prosemasher),
            outputs: vec![String::from(DRAFT_PATH)],
        },
        max_iterations: config.max_iterations,
        agent_defaults,
    })
}

/// Run the writing preset directly.
///
/// # Errors
///
/// Returns any error from building or running the underlying verified step.
pub async fn run_writing_step(
    executor: &Executor,
    config: &WritingStepConfig,
    agent_defaults: AgentConfig,
) -> Result<VerifiedStepResult, PipelineError> {
    let step = build_writing_step(config, agent_defaults)?;
    run_verified_step(executor, &config.work_dir, step).await
}

/// Return the absolute path to a shipped writing prompt template.
fn writing_prompt_path(filename: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("prompts")
        .join("writing")
        .join(filename)
        .to_string_lossy()
        .into_owned()
}

/// Validate the caller-supplied workspace directory.
fn validate_workspace_dir(work_dir: &Path) -> Result<(), PipelineError> {
    if !work_dir.exists() {
        return Err(PipelineError::Config(format!(
            "writing preset work_dir does not exist: {}",
            work_dir.display()
        )));
    }
    if !work_dir.is_dir() {
        return Err(PipelineError::Config(format!(
            "writing preset work_dir is not a directory: {}",
            work_dir.display()
        )));
    }
    Ok(())
}

/// Discover top-level workspace entries to treat as opaque inputs.
///
/// The writing preset does not impose input shape. Instead, it enumerates the
/// caller's top-level files and directories and declares them as inputs so they
/// are copied into iteration directories and transported correctly for remote
/// execution.
#[allow(
    clippy::type_complexity,
    reason = "guardrail misfires on this simple Vec<String> workspace inventory helper"
)]
fn discover_workspace_inputs(
    work_dir: &Path,
    step_name: &str,
) -> Result<Vec<String>, PipelineError> {
    let mut inputs = Vec::new();
    let entries = crate::fs::read_dir(work_dir).map_err(|e| {
        PipelineError::Config(format!(
            "failed to read writing workspace {}: {e}",
            work_dir.display()
        ))
    })?;

    for entry_result in entries {
        let entry = entry_result.map_err(|e| {
            PipelineError::Config(format!(
                "failed to read writing workspace entry in {}: {e}",
                work_dir.display()
            ))
        })?;
        let path = entry.path();
        if !path.is_file() && !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == step_name {
            continue;
        }
        inputs.push(name);
    }

    inputs.sort();
    Ok(inputs)
}

/// Build the critic agent breaker.
fn build_critic_breaker(
    step_name: &str,
    critic_prompt: &str,
    workspace_inputs: &[String],
) -> Breaker {
    let mut inputs = workspace_inputs.to_vec();
    inputs.push(String::from(DRAFT_PATH));

    Breaker::Agent {
        name: String::from("critic"),
        step: PromptedStep {
            name: format!("{step_name}-critic"),
            prompt_template: writing_prompt_path("critic.md"),
            vars: vec![
                Var::String {
                    placeholder: String::from("{{CRITIC_PROMPT}}"),
                    value: critic_prompt.to_owned(),
                },
                Var::String {
                    placeholder: String::from("{{DRAFT_PATH}}"),
                    value: String::from(DRAFT_PATH),
                },
                Var::String {
                    placeholder: String::from("{{OUTPUT_PATH}}"),
                    value: String::from(CRITIC_REPORT_PATH),
                },
            ],
            inputs,
            outputs: vec![String::from(CRITIC_REPORT_PATH)],
        },
    }
}

/// Build the optional `ProseSmasher` script breaker.
fn build_prosemasher_breaker() -> Breaker {
    Breaker::Script {
        name: String::from("prosemasher"),
        func: Arc::new(|iteration_dir: &Path| run_prosemasher_breaker(iteration_dir)),
    }
}

/// Run `ProseSmasher` against the current draft in an iteration directory.
fn run_prosemasher_breaker(iteration_dir: &Path) -> Result<(), String> {
    let draft_path = iteration_dir.join(DRAFT_PATH);
    let report_path = iteration_dir.join(PROSESMASHER_REPORT_PATH);
    if !draft_path.is_file() {
        let message = format!(
            "draft file not found for prosemasher: {}",
            draft_path.display()
        );
        write_prosesmasher_diagnostic_report(&report_path, None, "", &message)
            .map_err(|e| format!("failed to write {}: {e}", report_path.display()))?;
        return Err(message);
    }

    let output = run_prosesmasher_command(&draft_path)
        .map_err(|e| format!("failed to start prosesmasher: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

    #[allow(
        clippy::disallowed_methods,
        reason = "prosesmasher emits dynamic JSON; this breaker stores and forwards the raw report instead of validating into a fixed schema"
    )]
    let Ok(report) = serde_json::from_slice::<serde_json::Value>(output.stdout.as_slice()) else {
        write_prosesmasher_diagnostic_report(&report_path, output.status.code(), &stdout, &stderr)
            .map_err(|e| format!("failed to write {}: {e}", report_path.display()))?;
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            String::from("prosesmasher produced no parseable JSON output")
        };
        return Err(detail);
    };

    let pretty = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("failed to format prosesmasher JSON: {e}"))?;
    crate::fs::write(&report_path, &pretty)
        .map_err(|e| format!("failed to write {}: {e}", report_path.display()))?;

    if output.status.success() && prosemasher_report_is_clean(&report) {
        return Ok(());
    }

    if !output.status.success() && prosemasher_report_is_clean(&report) {
        return Err(format!(
            "prosesmasher exited {:?} despite a clean report:\n```json\n{pretty}\n```",
            output.status.code()
        ));
    }

    Err(format!(
        "ProseSmasher reported issues:\n```json\n{pretty}\n```"
    ))
}

/// Run the local `prosesmasher` CLI, falling back to the sibling workspace.
fn run_prosesmasher_command(draft_path: &Path) -> Result<std::process::Output, std::io::Error> {
    let draft_path_arg = draft_path.display().to_string();

    #[allow(
        clippy::disallowed_methods,
        reason = "script breakers are synchronous today; this CLI integration needs a direct subprocess call"
    )]
    match Command::new("prosesmasher")
        .args([
            "check",
            draft_path_arg.as_str(),
            "--preset",
            PROSESMASHER_PRESET,
            "--format",
            "json",
        ])
        .output()
    {
        Ok(output) => Ok(output),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let workspace = sibling_prosesmasher_workspace();
            #[allow(
                clippy::disallowed_methods,
                reason = "fallback to sibling workspace keeps local deterministic checks runnable without a prior install"
            )]
            Command::new("cargo")
                .current_dir(workspace)
                .args([
                    "run",
                    "-q",
                    "-p",
                    "prosesmasher",
                    "--",
                    "check",
                    draft_path_arg.as_str(),
                    "--preset",
                    PROSESMASHER_PRESET,
                    "--format",
                    "json",
                ])
                .output()
        }
        Err(error) => Err(error),
    }
}

/// Locate the sibling `prosesmasher` workspace checkout.
fn sibling_prosesmasher_workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("prosesmasher")
        .join("apps")
        .join("prosesmasher")
}

/// Persist a machine-readable diagnostic report when the CLI does not return JSON.
fn write_prosesmasher_diagnostic_report(
    report_path: &Path,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<(), PipelineError> {
    let report = serde_json::json!({
        "success": false,
        "exit_reason": "process-error",
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });
    let pretty = serde_json::to_string_pretty(&report).map_err(|e| {
        PipelineError::Other(format!(
            "failed to serialize prosesmasher diagnostic report: {e}"
        ))
    })?;
    crate::fs::write(report_path, pretty)?;
    Ok(())
}

/// Heuristic pass/fail check for `ProseSmasher` JSON output.
fn prosemasher_report_is_clean(report: &serde_json::Value) -> bool {
    if let Some(failures) = report.get("failures").and_then(serde_json::Value::as_array) {
        return failures.is_empty();
    }

    if let Some(failed) = report.get("failed").and_then(serde_json::Value::as_u64) {
        return failed == 0;
    }

    let has_non_empty_array = |key: &str| {
        report
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| !items.is_empty())
    };
    let has_empty_array = |key: &str| {
        report
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(std::vec::Vec::is_empty)
    };

    if report
        .get("passed")
        .and_then(serde_json::Value::as_bool)
        .is_some_and(|value| value)
        || report
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .is_some_and(|value| value)
        || report
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .is_some_and(|value| value)
    {
        return true;
    }

    for key in ["issues", "findings", "problems", "violations"] {
        if has_non_empty_array(key) {
            return false;
        }
    }

    for key in ["issues", "findings", "problems", "violations"] {
        if has_empty_array(key) {
            return true;
        }
    }

    false
}

/// Build the fixer's declared inputs.
fn fixer_inputs(workspace_inputs: &[String], use_prosemasher: bool) -> Vec<String> {
    let mut inputs = workspace_inputs.to_vec();
    inputs.push(String::from(DRAFT_PATH));
    inputs.push(String::from("issues.md"));
    inputs.push(String::from(CRITIC_REPORT_INPUT_PATH));
    if use_prosemasher {
        inputs.push(String::from(PROSESMASHER_REPORT_PATH));
    }
    inputs
}

#[cfg(test)]
#[path = "writing_tests.rs"]
mod tests;
