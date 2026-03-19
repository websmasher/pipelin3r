//! Orchestrator for the doer-breaker-fixer verification loop.
//!
//! Creates iteration directories, runs agents, collects issues, and
//! manages the convergence loop. See [`run_verified_step`] for details.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::AgentConfig;
use crate::error::PipelineError;
use crate::executor::Executor;
use crate::pool::run_pool_map;

use super::{Breaker, VerifiedStep, VerifiedStepResult};

/// Run a verified step with the doer-breaker-fixer convergence loop.
///
/// # Directory structure
///
/// ```text
/// {work_dir}/{step.name}/
///   iter-0/                    # doer
///     {doer inputs}            # copied from work_dir
///     {doer outputs}           # produced by doer
///   iter-1/                    # first breaker→fixer cycle
///     {output to check}        # copied from iter-0 (doer output)
///     issues.md                # combined breaker findings
///     {fixer outputs}          # produced by fixer
///   iter-2/                    # second cycle (if needed)
///     {output to check}        # renamed from iter-1 fixer output
///     issues.md
///     {fixer outputs}
///   final/                     # copy of last good outputs
/// ```
///
/// # Flow
///
/// 1. Doer runs once in `iter-0/`
/// 2. Breakers check doer output (scripts first, then agents)
/// 3. If no issues → copy to `final/`, return converged
/// 4. If issues → write `issues.md`, run fixer in next `iter-N/`
/// 5. Breakers check fixer output
/// 6. Repeat 4-5 until converged or `max_iterations` exceeded
///
/// # File routing
///
/// Fixer inputs are resolved with fallback: first from the current
/// iteration directory, then from `work_dir`. This allows the fixer
/// to access both the output being fixed AND original pipeline files
/// (like spec documents or disagreement data).
///
/// The fixer's outputs are copied to `final/` using the doer's output
/// names, so downstream steps see consistent filenames.
///
/// # Errors
///
/// Returns an error if any agent call fails with an unrecoverable error
/// (transport, auth, etc.). Breaker findings are NOT errors — they drive
/// the fixer loop. If iterations are exhausted without convergence, the
/// result has `converged: false` but is not an error.
#[allow(clippy::too_many_lines, reason = "orchestration with iteration loop")]
pub async fn run_verified_step(
    executor: &Executor,
    work_dir: &Path,
    step: VerifiedStep,
) -> Result<VerifiedStepResult, PipelineError> {
    let step_dir = work_dir.join(&step.name);
    crate::fs::create_dir_all(&step_dir)?;

    // ── Iteration 0: Doer ───────────────────────────────────────────
    let iter_0 = step_dir.join("iter-0");
    crate::fs::create_dir_all(&iter_0)?;

    tracing::info!(step = %step.name, "running doer in iter-0");

    // Copy doer inputs into iter-0 from work_dir.
    copy_inputs_required(work_dir, &iter_0, &step.doer.inputs, "doer")?;

    // Resolve and run doer.
    let doer_agent_step = step.doer.resolve(&iter_0)?;
    let doer_config = AgentConfig {
        name: doer_agent_step.config.name,
        prompt: doer_agent_step.config.prompt,
        work_dir: Some(iter_0.clone()),
        expect_outputs: step.doer.outputs.clone(),
        ..step.agent_defaults.clone()
    };
    let doer_result = executor.run_agent(&doer_config).await?;
    let _ = doer_result.require_success()?;

    // Verify doer outputs exist.
    verify_outputs_exist(&iter_0, &step.doer.outputs, "doer")?;

    // ── Breaker-fixer loop ──────────────────────────────────────────
    // Track: which dir has the current output, and which output names to use.
    let mut current_output_dir = iter_0;
    // After doer: outputs have doer's names. After fixer: outputs have
    // fixer's names. We track the "current" output file names for breakers.
    let mut current_output_names: Vec<String> = step.doer.outputs.clone();
    let mut iteration: usize = 0;

    loop {
        // Run breakers against current output.
        tracing::info!(
            step = %step.name,
            iteration,
            "running breakers"
        );
        let issues = run_breakers(
            executor,
            &current_output_dir,
            &step.breakers,
            &step.agent_defaults,
            &step.name,
            iteration,
        )
        .await?;

        if issues.is_empty() {
            // Converged — copy final outputs.
            tracing::info!(step = %step.name, iteration, "converged");
            let final_dir = copy_final_outputs(
                &current_output_dir,
                &step_dir,
                &current_output_names,
                &step.doer.outputs,
            )?;
            return Ok(VerifiedStepResult {
                converged: true,
                iterations: iteration,
                final_output_dir: final_dir,
                name: step.name.clone(),
            });
        }

        // Check iteration budget.
        iteration = iteration.saturating_add(1);
        if iteration > step.max_iterations {
            tracing::warn!(
                step = %step.name,
                iteration,
                "max iterations exceeded, not converged"
            );
            let final_dir = copy_final_outputs(
                &current_output_dir,
                &step_dir,
                &current_output_names,
                &step.doer.outputs,
            )?;
            return Ok(VerifiedStepResult {
                converged: false,
                iterations: iteration.saturating_sub(1),
                final_output_dir: final_dir,
                name: step.name.clone(),
            });
        }

        // ── Run fixer ───────────────────────────────────────────────
        let iter_dir = step_dir.join(format!("iter-{iteration}"));
        crate::fs::create_dir_all(&iter_dir)?;

        tracing::info!(step = %step.name, iteration, "running fixer");

        // Write combined issues file.
        let issues_content = format_issues(&issues);
        crate::fs::write(&iter_dir.join("issues.md"), &issues_content)?;

        // Copy fixer inputs with fallback chain:
        // 1. iter_dir itself (for issues.md already written above)
        // 2. current_output_dir (previous iteration's outputs)
        // 3. work_dir (original pipeline files like spec, disagreements)
        copy_inputs_with_fallback(
            &iter_dir,
            &current_output_dir,
            work_dir,
            &step.fixer.inputs,
            "fixer",
        )?;

        // Resolve and run fixer.
        let fixer_agent_step = step.fixer.resolve(&iter_dir)?;
        let fixer_config = AgentConfig {
            name: fixer_agent_step.config.name,
            prompt: fixer_agent_step.config.prompt,
            work_dir: Some(iter_dir.clone()),
            expect_outputs: step.fixer.outputs.clone(),
            ..step.agent_defaults.clone()
        };
        let fixer_result = executor.run_agent(&fixer_config).await?;
        let _ = fixer_result.require_success()?;

        // Verify fixer outputs exist.
        verify_outputs_exist(&iter_dir, &step.fixer.outputs, "fixer")?;

        current_output_dir = iter_dir;
        current_output_names = step.fixer.outputs.clone();
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Copy declared input files from `src_dir` to `dst_dir`. Fails if any
/// input is missing — doer inputs are required declarations.
///
/// Inputs can be files (`"spec.md"`) or directories (`"research/rust/"` —
/// all files are copied recursively).
fn copy_inputs_required(
    src_dir: &Path,
    dst_dir: &Path,
    inputs: &[String],
    role: &str,
) -> Result<(), PipelineError> {
    for input in inputs {
        let src = src_dir.join(input);
        let dst = dst_dir.join(input);

        if src.is_dir() {
            copy_dir_recursive(&src, &dst)?;
            continue;
        }

        if dst.is_file() {
            continue;
        }

        if !src.is_file() {
            return Err(PipelineError::Config(format!(
                "{role}: required input not found: {input} (in {})",
                src_dir.display()
            )));
        }

        if let Some(parent) = dst.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        let _ = crate::fs::copy(&src, &dst)?;
    }
    Ok(())
}

/// Copy fixer inputs with a fallback chain of source directories.
///
/// For each input, tries in order:
/// 1. `iter_dir` — already present (e.g., `issues.md` written before this call)
/// 2. `prev_iter_dir` — previous iteration's outputs
/// 3. `work_dir` — original pipeline files (spec, disagreements, etc.)
///
/// Inputs can be files or directories (copied recursively).
/// Fails if an input cannot be found in any source.
fn copy_inputs_with_fallback(
    iter_dir: &Path,
    prev_iter_dir: &Path,
    work_dir: &Path,
    inputs: &[String],
    role: &str,
) -> Result<(), PipelineError> {
    for input in inputs {
        let dst = iter_dir.join(input);

        // Already present in iter_dir (file or directory).
        if dst.is_file() || dst.is_dir() {
            continue;
        }

        // Try previous iteration dir (file or directory).
        let from_prev = prev_iter_dir.join(input);
        if from_prev.is_dir() {
            copy_dir_recursive(&from_prev, &dst)?;
            continue;
        }
        if from_prev.is_file() {
            if let Some(parent) = dst.parent() {
                crate::fs::create_dir_all(parent)?;
            }
            let _ = crate::fs::copy(&from_prev, &dst)?;
            continue;
        }

        // Try work_dir (original pipeline files).
        let from_work = work_dir.join(input);
        if from_work.is_dir() {
            copy_dir_recursive(&from_work, &dst)?;
            continue;
        }
        if from_work.is_file() {
            if let Some(parent) = dst.parent() {
                crate::fs::create_dir_all(parent)?;
            }
            let _ = crate::fs::copy(&from_work, &dst)?;
            continue;
        }

        return Err(PipelineError::Config(format!(
            "{role}: required input not found: {input} (checked {}, {}, {})",
            iter_dir.display(),
            prev_iter_dir.display(),
            work_dir.display(),
        )));
    }
    Ok(())
}

/// Verify that all declared outputs exist on disk.
fn verify_outputs_exist(dir: &Path, outputs: &[String], role: &str) -> Result<(), PipelineError> {
    for output in outputs {
        let path = dir.join(output);
        if !path.is_file() {
            return Err(PipelineError::Config(format!(
                "{role}: expected output not found: {output} (in {})",
                dir.display()
            )));
        }
    }
    Ok(())
}

/// A single breaker's findings.
struct BreakerIssue {
    /// Section header (breaker name).
    name: String,
    /// The issues text.
    text: String,
}

/// Run all breakers in sequence. Returns collected issues (empty = passed).
async fn run_breakers(
    executor: &Executor,
    output_dir: &Path,
    breakers: &[Breaker],
    agent_defaults: &AgentConfig,
    step_name: &str,
    iteration: usize,
) -> Result<Vec<BreakerIssue>, PipelineError> {
    let mut issues: Vec<BreakerIssue> = Vec::new();

    for breaker in breakers {
        match breaker {
            Breaker::Script { name, func } => {
                tracing::debug!(
                    step = step_name,
                    iteration,
                    breaker = name.as_str(),
                    "running script breaker"
                );
                // Script breakers get the output directory path.
                // They inspect files within it and return issues or Ok.
                if let Err(issue_text) = func(output_dir) {
                    tracing::info!(
                        step = step_name,
                        breaker = name.as_str(),
                        "script breaker found issues"
                    );
                    issues.push(BreakerIssue {
                        name: name.clone(),
                        text: issue_text,
                    });
                }
            }
            Breaker::Agent { name, step } => {
                tracing::debug!(
                    step = step_name,
                    iteration,
                    breaker = name.as_str(),
                    "running agent breaker"
                );

                // Create a subdirectory for the breaker agent to work in.
                let breaker_dir = output_dir.join(format!("breaker-{name}"));
                crate::fs::create_dir_all(&breaker_dir)?;

                // Copy breaker inputs from output_dir.
                copy_inputs_required(output_dir, &breaker_dir, &step.inputs, "breaker")?;

                // Resolve and run.
                let agent_step = step.resolve(&breaker_dir)?;
                let config = AgentConfig {
                    name: agent_step.config.name,
                    prompt: agent_step.config.prompt,
                    work_dir: Some(breaker_dir.clone()),
                    expect_outputs: step.outputs.clone(),
                    ..agent_defaults.clone()
                };
                let result = executor.run_agent(&config).await?;
                let _ = result.require_success()?;

                // Read the first output file as the issues text.
                if let Some(output_name) = step.outputs.first() {
                    let issues_path = breaker_dir.join(output_name);
                    if issues_path.is_file() {
                        let content = crate::fs::read_to_string(&issues_path).map_err(|e| {
                            PipelineError::Transport(format!(
                                "failed to read breaker output {}: {e}",
                                issues_path.display()
                            ))
                        })?;
                        let trimmed = content.trim();
                        if !trimmed.is_empty() && !is_no_issues(trimmed) {
                            tracing::info!(
                                step = step_name,
                                breaker = name.as_str(),
                                "agent breaker found issues"
                            );
                            issues.push(BreakerIssue {
                                name: name.clone(),
                                text: content,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(issues)
}

/// Check if breaker output indicates no issues were found.
///
/// Only matches exact sentinel phrases, not substrings, to avoid
/// false positives from text like "There were no issues with X, but Y..."
fn is_no_issues(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower == "no issues found"
        || lower == "no issues found."
        || lower == "no issues"
        || lower == "no issues."
        || lower == "lgtm"
        || lower == "lgtm."
        || lower == "approved"
        || lower == "approved."
}

/// Format collected issues into a single markdown document.
fn format_issues(issues: &[BreakerIssue]) -> String {
    let mut out = String::new();
    for (idx, issue) in issues.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str("## ");
        out.push_str(&issue.name);
        out.push('\n');
        out.push_str(&issue.text);
        out.push('\n');
    }
    out
}

/// Recursively copy a directory and all its contents.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), PipelineError> {
    crate::fs::create_dir_all(dst)?;
    let entries = crate::fs::read_dir(src).map_err(|e| {
        PipelineError::Transport(format!("failed to read directory {}: {e}", src.display()))
    })?;
    for entry_result in entries {
        let entry = entry_result.map_err(|e| {
            PipelineError::Transport(format!(
                "failed to read directory entry in {}: {e}",
                src.display()
            ))
        })?;
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dst_path)?;
        } else if entry_path.is_file() {
            let _ = crate::fs::copy(&entry_path, &dst_path)?;
        }
        // Skip symlinks and other special file types.
    }
    Ok(())
}

/// Copy final outputs to `{step_dir}/final/`.
///
/// `source_names` are the filenames to read from `source_dir`.
/// `final_names` are the filenames to write in the final directory.
/// This handles renaming fixer outputs (e.g., `rulings-fixed.json`)
/// back to the doer's output names (e.g., `rulings.json`).
fn copy_final_outputs(
    source_dir: &Path,
    step_dir: &Path,
    source_names: &[String],
    final_names: &[String],
) -> Result<PathBuf, PipelineError> {
    let final_dir = step_dir.join("final");
    crate::fs::create_dir_all(&final_dir)?;

    // Copy each source output, renaming to the doer's canonical name.
    // If source_names and final_names have different lengths, copy what we can.
    let count = std::cmp::min(source_names.len(), final_names.len());
    for idx in 0..count {
        if let (Some(src_name), Some(dst_name)) = (source_names.get(idx), final_names.get(idx)) {
            let src = source_dir.join(src_name);
            if src.is_file() {
                let dst = final_dir.join(dst_name);
                if let Some(parent) = dst.parent() {
                    crate::fs::create_dir_all(parent)?;
                }
                let _ = crate::fs::copy(&src, &dst)?;
            }
        }
    }

    Ok(final_dir)
}

/// Run multiple verified steps concurrently with bounded concurrency.
///
/// For each item, calls the mapper to produce a [`VerifiedStep`], then
/// runs all steps concurrently with the given concurrency limit. Each
/// item gets its own independent doer-breaker-fixer loop.
///
/// Returns per-item results paired with the original items.
pub async fn run_verified_step_batch<T, F>(
    executor: Arc<Executor>,
    work_dir: &Path,
    items: Vec<T>,
    concurrency: usize,
    mapper: F,
) -> Vec<(T, Result<VerifiedStepResult, PipelineError>)>
where
    T: Send + 'static,
    F: Fn(&T) -> VerifiedStep + Send + Sync + Clone + 'static,
{
    let total = items.len();
    let work_dir = work_dir.to_path_buf();

    run_pool_map(items, concurrency, total, move |item, idx, total| {
        let executor = Arc::clone(&executor);
        let work_dir = work_dir.clone();
        let mapper = mapper.clone();

        async move {
            let step = mapper(&item);
            let step_name = step.name.clone();

            tracing::info!(
                "[{}/{}] Running verified step: {}",
                idx.saturating_add(1),
                total,
                step_name
            );

            let result = run_verified_step(&executor, &work_dir, step).await;

            match &result {
                Ok(r) if r.converged => {
                    tracing::info!(
                        "[{}/{}] OK {} (converged in {} iterations)",
                        idx.saturating_add(1),
                        total,
                        step_name,
                        r.iterations,
                    );
                }
                Ok(r) => {
                    tracing::warn!(
                        "[{}/{}] {} did not converge after {} iterations",
                        idx.saturating_add(1),
                        total,
                        step_name,
                        r.iterations,
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "[{}/{}] FAILED {}: {e}",
                        idx.saturating_add(1),
                        total,
                        step_name,
                    );
                }
            }

            (item, result)
        }
    })
    .await
}
