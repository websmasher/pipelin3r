# Writing CLI Version Surface And Repo Cleanup

**Date:** 2026-03-26 13:44
**Scope:** `.gitignore`, `AGENTS.md`, `CLAUDE.md`, `.plans/2026-03-21-211621-parser-dev-pipeline-v4.md`, `packages/pipelin3r/build.rs`, `packages/pipelin3r/src/bin/pipeliner.rs`, `packages/pipelin3r/src/agent/execute.rs`, `packages/pipelin3r/src/presets/writing.rs`, `packages/pipelin3r/src/verified/orchestrator.rs`, `packages/pipelin3r/src/fs.rs`, `packages/pipelin3r/prompts/writing/writer.md`, `packages/pipelin3r/prompts/writing/rewriter.md`, `packages/pipelin3r/src/presets/writing_tests.rs`, `packages/pipelin3r/tests/writing_real_bundle.rs`

## Summary
Grouped the outstanding writing-step work into a single cleanup pass so the repository can be committed from a clean state. Added a real CLI version surface for `pipeliner`, fixed dry-run placeholder behavior to match the stricter non-empty output validation, and kept the previously-debugged writing preset/runtime changes together in one commit-ready unit.

## Context & Problem
The repository had accumulated a mix of meaningful writing-step changes plus supporting docs and index refreshes, but the CLI still could not identify its own build and the repo was not clean enough to hand to another project as a stable binary boundary. The user explicitly wanted the repo cleaned, grouped, committed, and the CLI rebuilt so another project can call it directly. During verification, the package also exposed a dry-run regression: expected outputs were created as zero-byte placeholders, which now failed the non-empty output checks introduced during the remote-writing runtime fixes.

## Decisions Made

### Add version output at the CLI boundary
- **Chose:** Add a build script that embeds git commit metadata into the `pipeliner` binary and expose it through `pipeliner --version`.
- **Why:** The current crate version `0.1.0` is not enough to identify a locally built binary from a dirty worktree. The operational need is to know which commit a binary came from, especially when another repo shells out to it.
- **Alternatives considered:**
  - Print only `CARGO_PKG_VERSION` — rejected because all local builds would still say `0.1.0`.
  - Query git at runtime — rejected because the binary may be called from another repo or from a copied path without its source checkout nearby.

### Keep the full writing-step change set together
- **Chose:** Commit the outstanding writing-step prompt, orchestration, preset, fixture, and CLI changes together instead of trying to split them into micro-commits retroactively.
- **Why:** The code paths are already coupled: input/output iteration layout, artifact-path support, prompt semantics, runtime reset behavior, and fixture expectations all changed as one feature line.
- **Alternatives considered:**
  - Split into separate runtime/prompt/docs commits now — rejected because the repo was already dirty and the immediate goal was to produce a clean, usable binary quickly.
  - Revert unrelated tracked file changes — rejected because the user asked to group and commit everything in the worktree rather than preserve a partially-dirty state.

### Fix dry-run to create non-empty expected outputs
- **Chose:** Make `execute_dry_run_capture()` write non-empty placeholder files for each expected output.
- **Why:** The orchestrator now rejects empty outputs, so the older dry-run behavior of writing empty files was inconsistent with real execution and broke the real-bundle integration test.
- **Alternatives considered:**
  - Special-case the output validator to tolerate zero-byte dry-run artifacts — rejected because that weakens the contract and hides real output-shape mistakes.
  - Remove the dry-run real-bundle test entirely — rejected because it still provides useful coverage of bundle copying and trace creation.

### Treat `.artifacts/` as local runtime debris, not source
- **Chose:** Ignore `.artifacts/` in git instead of trying to commit replay output or destructively delete it under the current tool policy.
- **Why:** The directory contains transient debug outputs from remote/local writing runs. It was useful during debugging but should not keep the repo dirty or become part of the source history.
- **Alternatives considered:**
  - Commit `.artifacts/` — rejected because runtime traces are not stable source inputs.
  - Delete `.artifacts/` with shell commands — rejected here because the environment blocked destructive removal commands.

## Architectural Notes
- `packages/pipelin3r/build.rs` now injects `PIPELIN3R_GIT_SHA` and `PIPELIN3R_GIT_DIRTY` at build time.
- `packages/pipelin3r/src/bin/pipeliner.rs` now has a real top-level `--version` path and updated usage text that includes `--artifact-path`.
- The writing preset remains artifact-centric and opaque-workspace-based; the CLI version work does not change its transport or iteration model.
- Dry-run now better matches the current verified-step contract: outputs are not merely declared, they are materialized as non-empty placeholder files.

## Information Sources
- `packages/pipelin3r/src/bin/pipeliner.rs` — CLI parsing, usage text, agent defaults
- `packages/pipelin3r/src/agent/execute.rs` — dry-run capture and expected output materialization
- `packages/pipelin3r/src/presets/writing.rs` — writing preset shape and default prompts
- `packages/pipelin3r/src/verified/orchestrator.rs` — iteration and final-output handling
- `packages/pipelin3r/tests/writing_real_bundle.rs` — real Steady Parent fixture contract
- `apps/shedul3r/MCP.md` — operational gotchas for remote file-writing tasks
- `.worklogs/2026-03-25-123549-writing-step-runnable.md`
- `.worklogs/2026-03-25-234748-writing-step-remote-learnings.md`
- `cargo test -p pipelin3r`
- `cargo clippy -p pipelin3r --all-targets -- -D warnings`
- `cargo deny check`

## Open Questions / Future Considerations
- The Quora-style answer flow still appears to stall in the critic stage on some real bundles. This cleanup commit does not claim to fix that.
- The CLI now reports commit metadata, but it still does not surface a richer build manifest such as build time or release channel.
- The `pipeliner write` help/version UX is improved, but the CLI still uses manual parsing and stringly usage text rather than a structured parser.

## Key Files for Context
- `packages/pipelin3r/src/bin/pipeliner.rs` — current CLI surface, including `--version`, usage text, and write command handling
- `packages/pipelin3r/build.rs` — build-time git metadata embedding
- `packages/pipelin3r/src/agent/execute.rs` — dry-run placeholder output behavior
- `packages/pipelin3r/src/presets/writing.rs` — writing preset config and output-path handling
- `packages/pipelin3r/src/verified/orchestrator.rs` — step directory lifecycle and output validation
- `packages/pipelin3r/tests/writing_real_bundle.rs` — real bundle integration expectations
- `apps/shedul3r/MCP.md` — remote execution gotchas that informed the writing runtime fixes
- `.worklogs/2026-03-25-123549-writing-step-runnable.md` — initial writing preset implementation context
- `.worklogs/2026-03-25-234748-writing-step-remote-learnings.md` — remote runtime fix context

## Next Steps / Continuation Plan
1. Trace the Quora critic-stage hang end to end using a real bundle, starting from `packages/pipelin3r/src/verified/orchestrator.rs` and `packages/pipelin3r/src/task/mod.rs`, and capture the real critic-task prompt/task YAML if needed.
2. Decide whether the writing preset should expose a first-class answer-style preset or whether Quora answers should continue using the generic writing preset with `--artifact-path answer.md`.
3. If another repo starts consuming the binary regularly, add a small machine-readable version output mode (for example `--version --json`) so wrappers can verify the exact build before invoking it.
