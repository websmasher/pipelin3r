# Make Writing Step Runnable End to End

**Date:** 2026-03-25 12:35
**Scope:** `packages/pipelin3r/src/presets/writing.rs`, `packages/pipelin3r/src/bin/pipeliner.rs`, `packages/pipelin3r/src/agent/execute.rs`, `packages/pipelin3r/src/executor/mod.rs`, `packages/pipelin3r/src/lib.rs`, writing prompt templates, related tests, `apps/f3tch/Cargo.toml`, `packages/pipelin3r/Cargo.toml`, `packages/shedul3r-rs-sdk/Cargo.toml`, `deny.toml`, and `Cargo.lock`

## Summary
Added a reusable writing-step preset and CLI wrapper on top of the existing verified-step orchestration, then fixed dry-run output handling so the preset actually runs through the same iteration/output contract as real execution. Updated adversarial and pipeline-context tests to match the corrected `expect_outputs` validation behavior, filled in missing `f3tch` package metadata so the repository guardrail hook would allow the commit, and aligned local dependency policy with `cargo-deny` by forcing `reqwest` onto `rustls` and allowing the `tokio` features the crate already uses.

## Context & Problem
The design discussion converged on a specialized writing step rather than another generic pipeline abstraction. The step had to accept only user prompts plus an opaque working directory, reuse the existing verified-step iteration model, and preserve the full step subtree (`iter-*`, `final/`) as the product output. The first pass was structurally correct but had a practical runtime gap: dry-run captures did not materialize expected output files, which broke the verified-step contract and made the CLI smoke path fail even though the transport/orchestrator model was otherwise correct.

## Decisions Made

### Writing step is a preset over `VerifiedStep`, not a new engine
- **Chose:** Implement `WritingStepConfig`, `build_writing_step`, and `run_writing_step` in a new `presets/writing.rs` module, plus a thin `pipeliner write` CLI.
- **Why:** The existing verified-step orchestrator already owns the hard parts: iteration directories, breaker/fixer loop, remote bundle upload/download, and final-output assembly. Replacing that would duplicate the transport semantics the user explicitly wanted preserved.
- **Alternatives considered:**
  - New standalone writing pipeline engine — rejected because it would reimplement orchestration that already works.
  - Force a typed bundle shape such as `writing.yaml` — rejected because the input contract was explicitly narrowed to prompt + opaque folder.

### Preserve the caller workspace as opaque top-level inputs
- **Chose:** Enumerate top-level entries in the caller's work dir and declare them as inputs for the writer/critic/fixer flow.
- **Why:** This keeps the user contract minimal while ensuring iteration directories and remote bundle uploads contain the same workspace context.
- **Alternatives considered:**
  - Require named inputs like `brief.md` and `research/` — rejected because it imposes structure the user does not want.
  - Let prompts refer to parent directories implicitly — rejected because verified-step copies declared inputs, and remote runs need explicit files in each iteration bundle.

### Make dry-run honor `expect_outputs`
- **Chose:** Extend `execute_dry_run_capture` to validate `expect_outputs`, create placeholder files under the iteration work dir, and return them in `output_files`.
- **Why:** Verified steps depend on declared outputs existing after each doer/breaker/fixer invocation. Without placeholder outputs, the writing preset failed in dry-run even though the orchestration path was otherwise valid.
- **Alternatives considered:**
  - Special-case writing preset to skip output existence checks in dry-run — rejected because it would fork runtime semantics and hide transport bugs.
  - Leave dry-run as metadata-only capture — rejected because it does not exercise the same output contract as real execution.

### Fix repository-level publish metadata that blocked the commit hook
- **Chose:** Add `description`, `license`, and `repository` to `apps/f3tch/Cargo.toml`.
- **Why:** Guardrail3 treats missing crates.io-required metadata on `f3tch` as error-level findings, so the repository could not be committed at all until those fields were present.
- **Alternatives considered:**
  - Bypass the hook — rejected because that would defeat the repo’s enforced guardrail policy.
  - Leave the repo in a non-committable state — rejected because the user explicitly asked for a commit.

### Align dependency policy with `cargo-deny`
- **Chose:** Disable default `reqwest` features in `pipelin3r` and `shedul3r-rs-sdk`, enable `rustls-tls`, add `rt`, `fs`, and `process` to the allowed `tokio` feature list in `deny.toml`, update `rustls-webpki`, and explicitly skip two unavoidable duplicate transitive crates in `cargo-deny`.
- **Why:** The commit hook surfaced a real mismatch between the repo’s dependency policy and the current crate manifests: default `reqwest` features were pulling in banned `native-tls` / `openssl`, while `pipelin3r` legitimately uses `tokio`'s `fs` and `process` features.
- **Alternatives considered:**
  - Suppress or bypass `cargo-deny` — rejected because it would leave the repository policy broken.
  - Remove `tokio::fs` / `tokio::process` usage from the current change — rejected because those features are already used elsewhere in the crate and are not introduced solely by this work.

### Keep ProseSmasher in the existing breaker model
- **Chose:** Treat ProseSmasher as an optional script breaker operating on `draft.md` in the current iteration dir, persisting its JSON stdout to `prosemasher-report.json`.
- **Why:** ProseSmasher is a CLI over a file path, which maps naturally onto the existing deterministic breaker shape while preserving evidence in the iteration tree.
- **Alternatives considered:**
  - Introduce a brand-new deterministic-breaker abstraction first — rejected for this pass because the existing script breaker was sufficient.
  - Make ProseSmasher a separate pipeline step outside verified-step — rejected because it needs to participate in the same convergence loop.

## Architectural Notes
The writing preset sits entirely above the existing verified-step / executor split:
- `run_verified_step` still owns `iter-*`, breaker subdirectories, and `final/`.
- `Executor::run_agent` still decides local vs remote transport per invocation.
- In remote mode, each iteration dir remains the bundle boundary.
- In dry-run mode, placeholder expected outputs now let the preset exercise the same declared-output contract as real execution.

The public reuse boundary is now:
- library: `run_writing_step(executor, config, agent_defaults)`
- CLI: `pipeliner write --workdir ... [prompt options]`

The user-visible output remains the full step subtree under `{work_dir}/{step_name}/`, not a single copied artifact.

## Information Sources
- `packages/pipelin3r/src/verified/orchestrator.rs` — iteration directory and final-output semantics
- `packages/pipelin3r/src/agent/execute.rs` — work-dir transport, dry-run capture, output handling
- `packages/pipelin3r/src/executor/mod.rs` — agent execution entrypoint
- `packages/pipelin3r/tests/pipeline_context.rs` — expectations around output copying and temp work dirs
- `packages/pipelin3r/tests/adversarial_work_dir.rs`
- `packages/pipelin3r/tests/adversarial_work_dir_round2.rs`
- `.plans/2026-03-25-writing-step-preset.md` — design notes captured during implementation
- `.worklogs/2026-03-19-124624-verified-step-and-v3-pipeline.md` — prior context on verified-step as the reusable convergence primitive
- `npx gitnexus analyze` on 2026-03-25 to refresh the stale index
- `npx gitnexus impact -r pipelin3r run_writing_step` — reported LOW risk with no upstream callers yet
- `npx gitnexus context -r pipelin3r --file packages/pipelin3r/src/agent/execute.rs execute_dry_run_capture` — showed direct incoming call from `run_agent`

## Open Questions / Future Considerations
- The CLI exists, but there is not yet a real non-dry-run end-to-end integration test against a live scheduler.
- The critic output is structured JSON, but the orchestrator still merges findings through the existing human-readable `issues.md` path rather than a typed issue model.
- ProseSmasher persistence currently piggybacks on the script breaker implementation rather than a first-class deterministic-artifact API.

## Key Files for Context
- `packages/pipelin3r/src/presets/writing.rs` — writing preset config and lowering into `VerifiedStep`
- `packages/pipelin3r/src/bin/pipeliner.rs` — CLI wrapper for `pipeliner write`
- `packages/pipelin3r/src/agent/execute.rs` — dry-run output placeholder behavior and remote output download path
- `packages/pipelin3r/src/executor/mod.rs` — `run_agent` wiring into dry-run / real execution
- `packages/pipelin3r/src/verified/orchestrator.rs` — authoritative iteration/final directory behavior
- `packages/pipelin3r/prompts/writing/writer.md` — writer prompt wrapper
- `packages/pipelin3r/prompts/writing/critic.md` — critic prompt wrapper and JSON output contract
- `packages/pipelin3r/prompts/writing/rewriter.md` — fixer prompt wrapper
- `packages/pipelin3r/tests/pipeline_context.rs` — output expectations for dry-run and remote temp dirs
- `packages/pipelin3r/tests/adversarial_work_dir.rs` — expect-output traversal regression coverage
- `packages/pipelin3r/tests/adversarial_work_dir_round2.rs` — additional expect-output validation edge cases
- `.plans/2026-03-25-writing-step-preset.md` — design plan for the preset
- `.worklogs/2026-03-19-124624-verified-step-and-v3-pipeline.md` — backstory on the verified-step abstraction

## Next Steps / Continuation Plan
1. Add a live end-to-end integration test that runs `pipeliner write` against a real local `shedul3r` instance so the remote/local transport path is exercised beyond dry-run.
2. Decide whether critic and ProseSmasher findings should converge on a typed issue artifact instead of only `issues.md`, then update the rewriter contract if needed.
3. If more presets are added, factor shared prompt-wrapping and opaque-workspace-input discovery into a reusable preset helper module instead of duplicating the pattern.
