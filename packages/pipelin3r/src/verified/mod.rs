//! Doer-breaker-fixer pattern for verified LLM pipeline steps.
//!
//! A [`VerifiedStep`] runs a doer agent once, then loops between breaker
//! checks and fixer agents until all breakers pass or `max_iterations` is
//! reached. Each iteration writes to its own directory — nothing is
//! overwritten, preserving a full chain of evidence for debugging.
//!
//! # Hierarchy
//!
//! ```text
//! PromptedStep  (template + vars → resolves to AgentStep)
//!   ↑ used by
//! VerifiedStep  (doer + breakers + fixer + loop)
//!   ↑ used by
//! run_verified_step  (orchestrator)
//! ```

mod orchestrator;

pub use orchestrator::{run_verified_step, run_verified_step_batch};

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::agent::AgentConfig;
use crate::error::PipelineError;
use crate::pipeline::AgentStep;
use crate::template::TemplateFiller;

// ── Template variables ──────────────────────────────────────────────────

/// A template variable for prompt resolution.
///
/// Used by [`PromptedStep`] to fill placeholders in a prompt template file.
#[derive(Debug, Clone)]
pub enum Var {
    /// Literal string replacement: `{{PLACEHOLDER}}` → `value`.
    String {
        /// The placeholder text to find (e.g., `"{{OUTPUT_PATH}}"`).
        placeholder: String,
        /// The literal value to substitute.
        value: String,
    },
    /// File content replacement: `{{PLACEHOLDER}}` → contents of file at `path`.
    ///
    /// The file is read from the iteration directory at resolve time, not
    /// from the pipeline base directory.
    File {
        /// The placeholder text to find (e.g., `"{{SPEC}}"`).
        placeholder: String,
        /// Relative path to the file whose content replaces the placeholder.
        path: String,
    },
}

// ── Prompted step ───────────────────────────────────────────────────────

/// An LLM step defined by a prompt template file and template variables.
///
/// Resolves to an [`AgentStep`] by loading the template, filling variables,
/// and attaching declared inputs/outputs. Use this for doer, breaker, and
/// fixer roles inside a [`VerifiedStep`].
///
/// # Template resolution
///
/// - [`Var::String`] values are passed to [`TemplateFiller::set`].
/// - [`Var::File`] values are read from the provided directory and passed
///   to [`TemplateFiller::set_content`].
#[derive(Debug, Clone)]
pub struct PromptedStep {
    /// Step name (for logging and directory naming).
    pub name: String,
    /// Path to the prompt template file on disk.
    pub prompt_template: String,
    /// Template variables to fill before execution.
    pub vars: Vec<Var>,
    /// Files this step reads (relative paths, copied into iteration dir).
    pub inputs: Vec<String>,
    /// Files this step produces (relative paths within iteration dir).
    pub outputs: Vec<String>,
}

impl PromptedStep {
    /// Resolve this prompted step into an executable [`AgentStep`].
    ///
    /// Loads the template from [`prompt_template`](Self::prompt_template),
    /// fills all [`vars`](Self::vars) (reading [`Var::File`] content from
    /// `context_dir`), and returns an [`AgentStep`] ready for execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the template file cannot be read, or if a
    /// [`Var::File`] target does not exist in `context_dir`.
    pub fn resolve(&self, context_dir: &Path) -> Result<AgentStep, PipelineError> {
        let template_path = std::path::Path::new(&self.prompt_template);
        let template_content = TemplateFiller::from_file(template_path)?;

        let mut filler = TemplateFiller::new();
        for var in &self.vars {
            match var {
                Var::String { placeholder, value } => {
                    filler = filler.set(placeholder, value);
                }
                Var::File { placeholder, path } => {
                    let file_path = context_dir.join(path);
                    let content = crate::fs::read_to_string(&file_path).map_err(|e| {
                        PipelineError::Template(format!(
                            "step '{}': failed to read Var::File '{}' from {}: {e}",
                            self.name,
                            path,
                            file_path.display()
                        ))
                    })?;
                    filler = filler.set_content(placeholder, &content);
                }
            }
        }

        let prompt = filler.fill(&template_content);

        Ok(AgentStep {
            config: AgentConfig::new(&self.name, prompt),
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
        })
    }
}

// ── Breaker ─────────────────────────────────────────────────────────────

/// A breaker that reviews output and finds issues.
///
/// Breakers run in sequence: script breakers first (fast, cheap), then
/// agent breakers (slow, expensive). All issues are collected into a
/// single `issues.md` file with section headers identifying each breaker.
pub enum Breaker {
    /// A deterministic validator function.
    ///
    /// Takes the path to the output file being checked. Returns `Ok(())`
    /// if no issues are found, or `Err(issues_text)` describing problems.
    /// Must NOT write files — only inspect and report.
    Script {
        /// Name for the issues section header (e.g., `"Format validation"`).
        name: String,
        /// The validation function.
        #[allow(
            clippy::type_complexity,
            reason = "fn trait object must be Arc-wrapped for Send+Sync"
        )]
        func: Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>,
    },
    /// An LLM agent that reviews output and writes an issues file.
    ///
    /// The agent's outputs should include a single issues file. If the
    /// file contains only whitespace or the text "No issues found", the
    /// breaker is considered to have passed.
    Agent {
        /// Name for the issues section header (e.g., `"Adversarial review"`).
        name: String,
        /// The agent step definition.
        step: PromptedStep,
    },
}

impl fmt::Debug for Breaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Script { name, .. } => f
                .debug_struct("Breaker::Script")
                .field("name", name)
                .finish_non_exhaustive(),
            Self::Agent { name, step } => f
                .debug_struct("Breaker::Agent")
                .field("name", name)
                .field("step", step)
                .finish(),
        }
    }
}

// ── Verified step ───────────────────────────────────────────────────────

/// A pipeline step with doer-breaker-fixer verification loop.
///
/// The doer runs once. If any breaker finds issues, the fixer runs with
/// the issues and current output. The loop continues between fixer and
/// breakers until all breakers pass or `max_iterations` is exhausted.
///
/// Each iteration writes to its own subdirectory under the step name,
/// preserving a full chain of evidence.
pub struct VerifiedStep {
    /// Step name (used for the top-level directory).
    pub name: String,
    /// The initial agent that produces output from scratch.
    pub doer: PromptedStep,
    /// Ordered list of breakers. Script breakers run first, then agents.
    pub breakers: Vec<Breaker>,
    /// The agent that fixes issues found by breakers.
    pub fixer: PromptedStep,
    /// Maximum number of fixer→breaker iterations (not counting the doer).
    pub max_iterations: usize,
    /// Agent configuration defaults applied to all agent calls (doer, breaker
    /// agents, fixer). The `name`, `prompt`, `work_dir`, and `expect_outputs`
    /// fields are overridden per-call — everything else (model, timeout,
    /// retry, provider, tools, auth) comes from here.
    pub agent_defaults: AgentConfig,
}

impl fmt::Debug for VerifiedStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedStep")
            .field("name", &self.name)
            .field("doer", &self.doer)
            .field("breakers_count", &self.breakers.len())
            .field("fixer", &self.fixer)
            .field("max_iterations", &self.max_iterations)
            .finish_non_exhaustive()
    }
}

/// Result of running a [`VerifiedStep`].
#[derive(Debug)]
pub struct VerifiedStepResult {
    /// Whether all breakers passed within the iteration budget.
    pub converged: bool,
    /// Total number of breaker→fixer iterations executed (0 if doer passed).
    pub iterations: usize,
    /// Path to the directory containing the final output files.
    pub final_output_dir: std::path::PathBuf,
    /// Step name (for error messages).
    pub name: String,
}

impl VerifiedStepResult {
    /// Return a reference to self if converged, or an error if not.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::VerificationFailed`] if the step did not
    /// converge within the iteration budget.
    pub fn require_converged(&self) -> Result<&Self, PipelineError> {
        if self.converged {
            Ok(self)
        } else {
            Err(PipelineError::VerificationFailed {
                name: self.name.clone(),
                iterations: self.iterations,
                final_issues: format!("did not converge after {} iterations", self.iterations),
            })
        }
    }
}

#[cfg(test)]
mod tests;
