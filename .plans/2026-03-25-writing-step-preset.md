# Writing Step Preset

**Date:** 2026-03-25
**Status:** Design agreed, ready for implementation

## Goal

Add a reusable writing preset on top of `pipelin3r`'s existing verified-step machinery so callers can run a full writer -> review -> rewrite convergence loop with only:

- a writer prompt
- a working directory
- optional prose analysis
- critic / rewriter prompts

The preset must preserve the current local/remote execution model, including per-iteration bundle upload/download and the full chain of evidence (`iter-*`, breaker dirs, `final/`).

## Non-Goals

- No user-facing input schema for the working directory
- No required `writing.yaml`
- No attempt to flatten output to a single final file
- No new execution engine separate from `VerifiedStep`

## Existing Code We Must Reuse

### Verified-step orchestration

`run_verified_step()` already implements the correct local iteration model:

- creates `{work_dir}/{step.name}/iter-0`
- runs doer in `iter-0`
- runs breakers against the current iteration dir
- writes `issues.md` into the next iteration dir
- runs fixer in `iter-N`
- copies final canonical outputs into `{step_dir}/final`

Reference:

- `packages/pipelin3r/src/verified/orchestrator.rs`

### Remote transport

`Executor::run_agent()` and `execute_with_work_dir()` already implement the correct transport model:

- local: pass iteration dir path directly as `working_directory`
- remote: upload the current iteration dir as a bundle
- run remotely inside the uploaded bundle dir
- download only declared `expect_outputs`
- delete the remote bundle afterward

Reference:

- `packages/pipelin3r/src/executor/mod.rs`
- `packages/pipelin3r/src/agent/execute.rs`

## User-Facing Contract

The writing preset takes:

- `writer_prompt`
- `work_dir`
- `use_prosemasher`
- `critic_prompt`
- `rewriter_prompt`
- optional execution config / executor
- optional `name`
- optional `max_iterations`

The working directory is opaque to the preset. It may contain any files or subdirectories. The preset does not impose naming or structure on user inputs.

The prompts are the contract. They tell the model how to interpret the folder contents.

## Output Contract

The output is the mutated working directory, specifically the verified-step subtree:

```text
{work_dir}/{step_name}/
  iter-0/
  iter-1/
  ...
  final/
```

This subtree is the product output. Users receive the full convergence trace, not just a final article file.

The preset may use internal canonical filenames inside each iteration directory, but those are implementation details of the preset, not required user input schema.

## Internal Iteration-Local Filenames

Inside each iteration directory, the preset uses fixed relative paths:

- `draft.md`
- `critic-report.json`
- `issues.md`
- optionally `prosemasher-report.json`

These paths must stay relative to the current iteration dir because:

- the doer/fixer `expect_outputs` mechanism depends on declared relative output paths
- remote upload/download operates on the current iteration dir as a bundle
- final output copying in `run_verified_step()` uses canonical doer output names

## Preset Flow

### 1. Writer doer

Runs once in `iter-0`.

Inputs:

- the user-supplied `writer_prompt`
- the current iteration dir contents

Behavior:

- inspect whatever files are present in the working directory copy
- write the draft to `draft.md`

### 2. Optional ProseSmasher script breaker

If `use_prosemasher = true`, run ProseSmasher against `draft.md` in the current iteration dir.

Facts:

- ProseSmasher is a CLI
- it takes a file path as input
- it emits JSON on stdout

Breaker behavior:

- locate `draft.md` in the current iteration dir
- invoke ProseSmasher on that file
- parse or normalize JSON stdout
- return `Ok(())` if there are no blocking findings
- return `Err(formatted_issues)` if there are issues

Optional enhancement:

- persist raw stdout JSON to `prosemasher-report.json` in the current iteration dir before returning issues text

For the first implementation, this can be done with the existing `Breaker::Script` even though the type only formally returns `Result<(), String>`.

### 3. Critic agent breaker

Runs in a breaker subdirectory inside the current iteration dir, as already implemented by `run_breakers()`.

Inputs:

- `draft.md`
- optionally `prosemasher-report.json` if present
- the user-supplied `critic_prompt`

Behavior:

- review the draft against the prompt's criteria
- produce a structured report at `critic-report.json`

The preset appends required output-shape instructions to the user prompt so the critic output is predictable and reusable by the rewriter.

### 4. Issue aggregation

The orchestrator already merges breaker findings into `issues.md`.

For the first implementation:

- ProseSmasher findings are converted to text by the script breaker
- critic findings are converted from `critic-report.json` to text or written as already formatted report text
- combined result goes into `issues.md`

### 5. Rewriter fixer

Runs in `iter-N`.

Inputs:

- `draft.md` from the previous iteration dir
- `issues.md`
- original working directory files via the existing fallback logic
- the user-supplied `rewriter_prompt`

Behavior:

- revise the current draft against the merged issues
- write the revised draft to the preset's canonical output path for that iteration

### 6. Finalization

On convergence or exhaustion:

- `run_verified_step()` copies the current canonical output into `{step_dir}/final/`
- that `final/` directory remains part of the returned step subtree

## Critic Output Shape

The critic prompt should be wrapped by the preset so it always emits structured JSON.

Target shape:

```json
{
  "passed": false,
  "summary": "Short overall verdict.",
  "issues": [
    {
      "id": "clarity-1",
      "severity": "error",
      "category": "clarity",
      "location_hint": "section 2, paragraph 1",
      "message": "The sentence is vague.",
      "suggested_fix": "State the concrete claim directly."
    }
  ]
}
```

This is the reusable contract between critic and rewriter.

The preset appends these requirements to the user-supplied critic prompt rather than forcing the user to remember them.

## Rewriter Prompt Wrapping

The preset wraps the user-supplied rewriter prompt with:

- where to read the current draft
- where to read the merged issues
- where to write the revised draft

This keeps the user prompt focused on editorial intent, while the preset owns the iteration-local file protocol.

## API Shape

### Library surface

```rust
pub struct WritingStepConfig {
    pub name: String,
    pub work_dir: PathBuf,
    pub writer_prompt: String,
    pub critic_prompt: String,
    pub rewriter_prompt: String,
    pub use_prosemasher: bool,
    pub max_iterations: usize,
}

pub async fn run_writing_step(
    executor: &Executor,
    config: WritingStepConfig,
) -> Result<VerifiedStepResult, PipelineError>;
```

Alternative helper:

```rust
pub fn build_writing_verified_step(
    config: &WritingStepConfig,
) -> VerifiedStep;
```

The builder form is useful if another pipeline wants to compose this preset into a larger graph.

### CLI surface

Possible CLI:

```bash
pipeliner write \
  --workdir /path/to/folder \
  --writer-prompt-file writer.md \
  --critic-prompt-file critic.md \
  --rewriter-prompt-file rewriter.md \
  --use-prosemasher
```

Remote execution should be handled by normal executor construction flags, not by inventing a separate transport system for the preset.

## Implementation Strategy

### Phase 1: Minimal preset using existing `Breaker::Script`

1. Add `presets/writing.rs`
2. Define `WritingStepConfig`
3. Build a `VerifiedStep` with:
   - doer output: `draft.md`
   - optional ProseSmasher script breaker
   - critic agent breaker
   - fixer output: canonical draft output
4. Reuse the existing orchestrator unchanged

This gets the preset working quickly with the current architecture.

### Phase 2: Improve deterministic breaker ergonomics

The current `Breaker::Script` type only returns `Result<(), String>`.

That is enough to surface ProseSmasher findings, but awkward if we want deterministic review artifacts to be first-class evidence.

Possible follow-up:

- extend breaker API to support deterministic artifact emission
- or formally bless script breakers that write extra files into the current iteration dir

This is a refinement, not required for the first implementation.

## Files to Add / Modify

### New files

- `packages/pipelin3r/src/presets/mod.rs`
- `packages/pipelin3r/src/presets/writing.rs`
- `packages/pipelin3r/src/presets/tests.rs` or `packages/pipelin3r/tests/writing_step.rs`

### Modified files

- `packages/pipelin3r/src/lib.rs` — export preset module
- optionally `packages/pipelin3r/src/verified/mod.rs` if helper constructors or wrapper types are added

## Open Questions

1. Should the critic breaker emit JSON directly or markdown that embeds JSON?
2. Should the preset preserve raw ProseSmasher JSON as `prosemasher-report.json` in iteration dirs from v1?
3. Should the preset canonical draft filename always be `draft.md`, or should the caller be allowed to override it?
4. Should the CLI live in this repo now, or should we first ship the library preset and add the CLI wrapper later?

## Recommended First Cut

Build the smallest useful thing:

- one library preset
- fixed canonical iteration-local filenames
- optional ProseSmasher script breaker
- structured critic output
- existing verified-step orchestration unchanged

That delivers a reusable writing unit without touching the proven iteration and transport model.
