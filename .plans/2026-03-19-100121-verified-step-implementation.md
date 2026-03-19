# Implement VerifiedStep (doer-breaker-fixer pattern)

**Date:** 2026-03-19 10:01
**Task:** Implement the verified step primitives from the pipeline design handoff document

## Goal
Add `Var`, `Breaker`, `VerifiedStep`, and `run_verified_step` to the pipelin3r package, enabling the doer-breaker-fixer convergence pattern with iteration directories and full chain of evidence.

## Input Information
- Design spec: `.plans/todo/new_pipeline_design.md`
- Existing `AgentStep` holds `AgentConfig` (prompt inline) + inputs/outputs
- Existing `TemplateFiller` does injection-safe replacement
- Existing `PipelineContext` handles local/remote file routing
- Existing `validate_and_fix` is a different pattern (validator + strategy) — we're building alongside it, not replacing it
- Strict clippy: no unwrap, no indexing, no bare arithmetic, no unsafe

## Approach

### Key design decision: evolve AgentStep vs new type

The handoff doc shows `AgentStep` with `prompt_template` + `vars` replacing the current `config: AgentConfig` approach. But the current `AgentStep` is already exported and used by `dev-process-v2`.

**Decision:** Create a new `PromptedStep` type for the template-based steps used in `VerifiedStep`. Keep existing `AgentStep` unchanged for backward compatibility. `PromptedStep` resolves to an `AgentStep` via a `resolve()` method that loads the template and fills vars.

Alternative considered: modify `AgentStep` to hold template + vars — rejected because it breaks existing consumers and conflates two levels of abstraction (template resolution vs execution).

### Step-by-step plan

1. **New file: `packages/pipelin3r/src/verified.rs`** (~350 lines)
   - `Var` enum: `String { placeholder, value }` and `File { placeholder, path }`
   - `PromptedStep` struct: `prompt_template`, `vars`, `inputs`, `outputs`, `name`
   - `PromptedStep::resolve(&self, work_dir: &Path) -> Result<AgentStep, PipelineError>` — loads template, fills vars (Var::File reads from work_dir)
   - `Breaker` enum: `Script(Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>)` and `Agent(PromptedStep)`
   - `VerifiedStep` struct: `name`, `doer: PromptedStep`, `breakers: Vec<Breaker>`, `fixer: PromptedStep`, `max_iterations: usize`
   - `VerifiedStepResult` struct: `converged`, `iterations`, `final_output_dir: PathBuf`

2. **New file: `packages/pipelin3r/src/verified/orchestrator.rs`** (~250 lines)
   - `run_verified_step(executor, work_dir, step) -> Result<VerifiedStepResult, PipelineError>`
   - Creates `{work_dir}/{name}/iter-0/`, copies doer inputs, runs doer
   - Runs breakers in sequence (script first, then agent), collects issues into `issues.md`
   - If issues: creates `iter-N/`, copies fixer inputs + issues, runs fixer
   - Loop fixer→breaker until converged or max_iterations
   - Copies final output to `{name}/{output}-final` (or symlink)

3. **Update `lib.rs`** — add `pub mod verified;` and re-exports

4. **Tests** — unit tests for Var resolution, PromptedStep::resolve, orchestrator with mock executor

### Directory structure produced by run_verified_step
```
{work_dir}/{step.name}/
  iter-0/                    # doer
    {inputs}                 # copied in
    {doer outputs}           # produced by doer
  iter-1/                    # first breaker + fixer cycle
    {output being checked}   # copied from iter-0
    issues.md                # combined breaker output
    {fixer outputs}          # produced by fixer
  iter-2/                    # second cycle (if needed)
    ...
  {output}-final             # copy of last good output
```

## Files to Modify
- `packages/pipelin3r/src/verified.rs` — NEW: types (Var, PromptedStep, Breaker, VerifiedStep)
- `packages/pipelin3r/src/verified/orchestrator.rs` — NEW: run_verified_step function
- `packages/pipelin3r/src/lib.rs` — add module + re-exports
- `packages/pipelin3r/src/error.rs` — possibly add VerifiedStepFailed variant

## Risks & Edge Cases
- Breaker script fn is not Send+Sync by default — wrap in Arc
- Var::File reads from iter dir, not base_dir — must be clear about resolution context
- Remote execution within verified step — each doer/breaker/fixer call uses PipelineContext's transport
- Max iterations = 0 should be treated as "doer only, no verification"
