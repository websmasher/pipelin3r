# pipelin3r work_dir redesign + guardrail3 compliance

**Date:** 2026-03-17 17:10
**Scope:** packages/pipelin3r/, packages/shedul3r-rs-sdk/, packages/limit3r/, guardrail3 config, apps/shedul3r/ module splits

## Summary

Redesigned pipelin3r's block interface around filesystem paths (`work_dir`) instead of in-memory `Bundle` objects. Fixed 16 security/correctness bugs found via 2 rounds of adversarial testing (63 adversarial tests). Resolved all guardrail3 clippy violations across the entire workspace.

## Context & Problem

Both real-world pipelines (websmasher/dev-process, steady-parent) follow the same pattern: assemble a directory → send prompt + directory to Claude Code via shedul3r → agent reads/writes files in that directory. The old API forced users to construct `Bundle` objects, manage `working_dir`/`expected_output`/`bundle_data` as separate concepts, and manually toggle `.remote()`. This was unnecessary abstraction — the workspace is just a folder on disk.

## Decisions Made

### Work_dir replaces Bundle as the user-facing concept
- **Chose:** Single `.work_dir(path)` method replacing `.working_dir()`, `.expected_output()`, `.bundle()`
- **Why:** A block operates on `(prompt, folder) → (response, folder)`. The folder is just a `PathBuf`. No wrapper needed — users assemble dirs with `std::fs`.
- **Alternatives considered:**
  - `WorkDir` wrapper type — rejected because it would just wrap `std::fs` operations for no reason
  - Keep `Bundle` as public — rejected because it forces users to think about transport instead of content

### Auto-detect local vs remote from URL
- **Chose:** `is_local()` checks if shedul3r URL is localhost/127.0.0.1/[::1]
- **Why:** Eliminates the `.remote()` flag. If shedul3r is local, pass the path. If remote, upload via bundle endpoints automatically.
- **Alternatives considered:**
  - Explicit `.remote()` flag — rejected as unnecessary when URL already tells us

### Bundle becomes internal transport mechanism
- **Chose:** `Bundle` struct removed entirely. Only `validate_path()` function retained. Module is `pub(crate)`.
- **Why:** Remote transport reads files from the work_dir via `read_dir_to_memory()` and uploads them. No need for an intermediate `Bundle` type.

## Security Fixes (from adversarial testing)

1. `is_local()` subdomain bypass (`localhost.evil.com`) — fixed with delimiter check
2. `is_local()` case-insensitive hostnames — fixed with `to_ascii_lowercase()`
3. `is_local()` URL credentials — fixed by stripping `user:pass@`
4. `expect_outputs` path traversal (`../../../etc/passwd`) — fixed with `validate_path()`
5. No work_dir validation (empty, relative, `..`, nonexistent) — added `validate_work_dir()`
6. Root `/` accepted as work_dir — rejected explicitly
7. Symlink escape in remote upload — canonical path check + boundary validation
8. Symlink directory loops — visited-set tracking prevents infinite recursion
9. Silent error swallowing on unreadable work_dir — errors now propagate
10. Shared work_dir in batch — duplicate detection with canonical paths
11. Batch dry-run skipped validation — now calls `validate_work_dir()`
12. Dry-run counter global not per-agent — now per-step-name via `BTreeMap`
13. Batch dry-run error propagation inconsistent — now matches real-mode per-item results

## Guardrail3 Compliance

Regenerated guardrail3 config (`rs init --force` + `rs generate`). Fixed all clippy violations across all 3 packages:
- limit3r: 37 errors (test allows for `disallowed_methods`, `significant_drop_tightening`)
- shedul3r-rs-sdk: 16 errors (type aliases, SDK-specific `#[allow]`s with reasons)
- pipelin3r: 49 errors (type aliases, `#[allow]`s for Mutex/env vars/fs ops with reasons)

## Key Files for Context

- `.plans/2026-03-17-151543-pipelin3r-workdir-redesign.md` — the design plan
- `packages/pipelin3r/src/agent/mod.rs` — AgentBuilder with `.work_dir()` API
- `packages/pipelin3r/src/agent/execute.rs` — transport logic, validation, dir reading
- `packages/pipelin3r/src/executor/mod.rs` — `is_local()` detection
- `packages/pipelin3r/tests/adversarial_work_dir.rs` — 25 adversarial tests (round 1)
- `packages/pipelin3r/tests/adversarial_work_dir_round2.rs` — 38 adversarial tests (round 2)

## Next Steps / Continuation Plan

1. Design and implement the mid-level composition patterns discussed earlier:
   - Structured output agent (parse JSON from response, retry with error feedback)
   - Batch-map step (discover items → per-item work_dir + prompt → bounded concurrency)
   - Validation + remediation loop (validate → fix via agent → revalidate)
2. Image generation block (OpenRouter API integration)
3. Consider whether step sequencing needs formalization or if linear `for` loops + `if` branches suffice
