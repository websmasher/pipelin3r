# Writing Step Remote Learnings And Runtime Fixes

**Date:** 2026-03-25 23:47
**Scope:** `packages/pipelin3r/src/verified/orchestrator.rs`, `packages/pipelin3r/src/task/mod.rs`, `packages/pipelin3r/src/task/tests.rs`, `packages/pipelin3r/src/executor/mod.rs`, `apps/shedul3r/MCP.md`

## Summary
Fixed the real remote failure mode for the writing preset by tracing the exact Steady Parent writer bundle through the live Railway worker. The main runtime fix is that agent tasks with declared outputs now treat stabilized output files as the success signal instead of waiting forever for `claude -p` to exit cleanly after it has already written the file.

## Context & Problem
The user asked for a real remote replay against the Steady Parent writer flow, not dry-run assertions. Earlier attempts failed with opaque `Exit 1:` errors and no useful remote logs. Reproducing against the real worker uncovered multiple overlapping problems: I was initially using the wrong Steady Parent input shape, reruns were reusing stale `iter-*` directories, the forwarded Claude OAuth token had expired, and even with fresh auth Claude would sometimes write the expected file and then keep the session open instead of exiting. That last behavior caused the scheduler to retry a task that had already produced the file.

## Decisions Made

### Reset verified-step state on rerun
- **Chose:** Delete `{work_dir}/{step_name}` before a new `run_verified_step()` execution.
- **Why:** Replays into the same work directory were inheriting stale `iter-*`, `breaker-*`, and `final/` artifacts. That made debugging unreliable and could contaminate later iterations with files from an older run.
- **Alternatives considered:**
  - Keep existing step trees and try to selectively overwrite files — rejected because stale iteration state is hard to reason about and easy to miss.
  - Require callers to manually delete step directories before every run — rejected because the orchestrator owns that lifecycle.

### Treat declared output files as the success condition for Claude file-writing tasks
- **Chose:** Wrap generated `claude -p` commands in a shell watcher when `expect_outputs` is non-empty. The wrapper buffers stdin to a temp file, runs Claude in the background, polls expected outputs until they exist and are non-empty/stable, then exits successfully even if Claude keeps the session open.
- **Why:** The real remote probe proved Claude was writing `draft.md` and then hanging. The scheduler only saw a non-zero/timeout exit, so it retried work that had already succeeded.
- **Alternatives considered:**
  - Only trust Claude's process exit code — rejected because it is false-negative for file-writing tasks.
  - Download remote outputs even on task failure — rejected as a partial fix that still leaves long hangs and unnecessary retries.
  - Add a writing-preset-specific timeout wrapper — rejected because the behavior is generic to agent tasks with declared outputs, not unique to writing.

### Capture Claude auth learning in the shedul3r MCP doc
- **Chose:** Add authentication and file-writing gotchas to `apps/shedul3r/MCP.md`.
- **Why:** The failure mode was operational, not obvious from code. Future agents need to know that a stale keychain token can surface as empty stderr plus an opaque scheduler failure, and that local `claude -p` refreshes the token before extraction.
- **Alternatives considered:**
  - Leave the learning only in the worklog — rejected because the MCP doc is the operational guide future agents will read first.
  - Put it only in the writing preset code comments — rejected because the auth issue is a shedul3r/Claude remote-execution concern.

## Architectural Notes
- `run_verified_step()` now owns replay hygiene by clearing the old step subtree up front.
- `build_task_yaml()` now has a file-watcher wrapper path for agent tasks with declared outputs. Raw shell commands still use `command_override` unchanged.
- `Executor::run_agent()` passes `expect_outputs` into task generation so the shell wrapper knows which files define success.
- The live Steady Parent replay proved the system boundary now works end to end:
  - correct assembled bundle shape (`prompt.md` + `writer.md`)
  - refreshed Claude OAuth token
  - remote Railway worker execution
  - local download of `iter-*` and `final/`

## Information Sources
- `apps/shedul3r/MCP.md` — auth and remote execution guidance
- `packages/pipelin3r/src/verified/orchestrator.rs` — step directory lifecycle
- `packages/pipelin3r/src/task/mod.rs` — generated Claude task command
- `packages/pipelin3r/src/executor/mod.rs` — agent task YAML generation
- `packages/pipelin3r/src/presets/writing.rs` — declared outputs and writing-step structure
- `packages/pipelin3r/tests/fixtures/steady-parent/...` plus real Steady Parent step-6 assembly logic in `/Users/tartakovsky/Projects/steady-parent/packages/generator/pipeline/6_write/scripts/generate.ts`
- Live remote probe against `https://claude-worker-pipelin3r-rest.trtk.me`

## Open Questions / Future Considerations
- The writing step now runs end to end and returns a real `final/`, but the tested article did not converge within `max_iterations=3`. Content quality/prompting still needs work separately from transport/runtime.
- The writing preset still hardcodes `draft.md` as the internal artifact path. The tested Steady Parent prompt expects `article.mdx`. That mismatch did not block runtime once Claude was watched by output-file presence, but it is still an awkward contract and should probably become configurable.
- The `.artifacts/steady-parent-writing/` replay output is useful for manual inspection but should stay uncommitted unless the team explicitly wants runtime artifacts in git.

## Key Files for Context
- `packages/pipelin3r/src/verified/orchestrator.rs` — verified-step replay reset and iteration control
- `packages/pipelin3r/src/task/mod.rs` — output-watcher Claude command generation
- `packages/pipelin3r/src/executor/mod.rs` — where agent `expect_outputs` are passed into task generation
- `packages/pipelin3r/src/presets/writing.rs` — writing preset shape and current internal artifact naming
- `packages/pipelin3r/src/task/tests.rs` — tests for the new output-watcher command path
- `apps/shedul3r/MCP.md` — operational learnings on auth refresh and file-writing task behavior
- `.worklogs/2026-03-25-123549-writing-step-runnable.md` — earlier implementation context for the writing preset
- `.worklogs/2026-03-19-124624-verified-step-and-v3-pipeline.md` — original verified-step design and remote execution backstory

## Next Steps / Continuation Plan
1. Make the writing preset’s internal output filename configurable so external prompt contracts like Steady Parent’s `article.mdx` do not fight the preset’s `draft.md`.
2. Improve the critic/rewriter loop so `prosesmasher` and critic findings actually reduce across iterations; the current replay reached `final/` but still failed several style checks.
3. If the team wants stable reproduction, replace the old Steady Parent fixture here with the real assembled step-6 bundle shape (`prompt.md` + filled `writer.md`) rather than the older giant-prompt bundle format.
