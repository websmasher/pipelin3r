# Verified step primitives + V3 pipeline + remote command execution

**Date:** 2026-03-19 12:46
**Scope:** packages/pipelin3r/src/verified/, packages/pipelin3r/src/executor/, packages/pipelin3r/src/task/, packages/pipelin3r/src/error.rs, packages/pipelin3r/src/lib.rs

## Summary
Added the doer-breaker-fixer verification pattern to pipelin3r (`VerifiedStep`, `PromptedStep`, `Var`, `Breaker`, `run_verified_step`, `run_verified_step_batch`). Added `RemoteCommandConfig` + `Executor::run_remote_command` for running raw shell commands on shedul3r without LLM involvement. Built and tested V3 pipeline (steps 1-8) in websmasher using the new primitives, running end-to-end remotely on Railway.

## Context & Problem
The pipelin3r library had low-level primitives (`run_agent`, `PipelineContext`) but no higher-level composition pattern for verified LLM steps. The handoff doc (`.plans/todo/new_pipeline_design.md`) specified a doer-breaker-fixer pattern where LLM outputs are verified by script or adversarial LLM breakers and iteratively fixed.

Additionally, all shedul3r task execution was hardcoded to Claude Code invocations (`claude -p --model ...`). Script steps (git clone, pip install, running parsers) required LLM wrapping to execute remotely, wasting API calls.

## Decisions Made

### 1. PromptedStep instead of modifying AgentStep
- **Chose:** New `PromptedStep` type for template-based steps, existing `AgentStep` unchanged
- **Why:** `AgentStep` is already exported and used by consumers. `PromptedStep` resolves to `AgentStep` via `.resolve()`.
- **Alternatives:** Modify AgentStep to hold template+vars — rejected (breaks existing consumers)

### 2. agent_defaults on VerifiedStep
- **Chose:** `VerifiedStep.agent_defaults: AgentConfig` applied to all agent calls
- **Why:** Without this, `PromptedStep::resolve()` creates bare `AgentConfig::new()` with all Nones — no model, no timeout, no retry. Discovered when first remote run showed `model=None` in logs.
- **Alternatives:** Defaults on PromptedStep — rejected (duplicated across doer/breaker/fixer)

### 3. TaskConfig.command_override for raw commands
- **Chose:** Optional `command_override: Option<String>` on `TaskConfig`. When set, used instead of building `claude -p ...` command.
- **Why:** shedul3r accepts any YAML — the command field is just `/bin/sh -c <string>`. No reason to wrap every command in Claude Code.
- **Alternatives:** Separate `CommandTaskConfig` type — rejected (more duplication, same YAML structure)

### 4. Everything runs on Railway, nothing installed locally
- **Chose:** All script steps (clone, install, run parsers) execute remotely via `run_remote_command`. Local machine is orchestrator only.
- **Why:** User doesn't want 21 libraries' language runtimes installed locally. Railway has persistent volumes.

## Architectural Notes

### Verified step hierarchy
```
PromptedStep (template + vars → resolves to AgentStep)
  ↑ used by
VerifiedStep (doer + breakers + fixer + agent_defaults + loop)
  ↑ used by
run_verified_step / run_verified_step_batch (orchestrator)
```

### Remote command execution
```
RemoteCommandConfig { command: "git clone ..." }
  → Executor::run_remote_command()
    → build_task_yaml (with command_override)
      → shedul3r runs /bin/sh -c "git clone ..."
```

### Iteration directory structure
Each VerifiedStep creates `{work_dir}/{name}/iter-0/`, `iter-1/`, etc. Nothing overwritten. Fixer inputs resolved via fallback chain: iter_dir → prev_iter_dir → work_dir.

## Bugs Found and Fixed
1. **PromptedStep::resolve() produced bare AgentConfig** — no model, timeout, retry. Fixed: added `agent_defaults` to VerifiedStep, applied via struct update syntax.
2. **OUTPUT_PATH mismatch** — prompt told agent to write `research/{slug}/overview.md` but expect_outputs was `overview.md`. Agent wrote to subdirectory, shedul3r 404'd on download. Fixed: set OUTPUT_PATH to just the filename.
3. **Fixer couldn't access base_dir files** — orchestrator only copied from prev iter dir. Fixed: `copy_inputs_with_fallback` tries iter_dir → prev_iter_dir → work_dir.
4. **copy_final_outputs used doer's output names after fixer ran** — fixer produces differently-named files. Fixed: track current_output_names, pass both source and final names to copy function.

## V3 Pipeline Results (steps 1-3 tested remotely)
- Step 1: architecture.md — converged in 0 iterations, 136 lines
- Step 2: 9/9 languages researched, all converged in 0 iterations
- Step 3: 21 libraries extracted (vs V2's 17), Java/Elixir correctly identified 0 parsers after 3 fixer iterations
- V3 found 4 more libraries than V2: secmap (Rust), python-securitytxt (Python), 3n3a-archive (Go), DomainDetective (C#)

## Key Files for Context
- `.plans/todo/new_pipeline_design.md` — original design spec for the verified step pattern
- `packages/pipelin3r/src/verified/mod.rs` — types (Var, PromptedStep, Breaker, VerifiedStep, VerifiedStepResult)
- `packages/pipelin3r/src/verified/orchestrator.rs` — run_verified_step, run_verified_step_batch
- `packages/pipelin3r/src/executor/mod.rs` — RemoteCommandConfig, run_remote_command
- `packages/pipelin3r/src/task/mod.rs` — command_override field
- V3 pipeline: `websmasher/tools/dev-process-v3/` (separate repo, not committed here)

## Next Steps / Continuation Plan
1. Attach a Railway volume at `/data/pipelin3r/` to the shedul3r service
2. Run V3 steps 4-8 end-to-end remotely (clone, install, wrappers, run parsers, classify)
3. Step 6 (research spec) is the first step with an adversarial LLM breaker — test the doer→script breaker→agent breaker→fixer loop
4. Steps 7-8 depend on step 4's wrappers working — may need iteration on wrapper generation prompts
5. The V3 pipeline lives in websmasher repo, not pipelin3r — commit it there separately
