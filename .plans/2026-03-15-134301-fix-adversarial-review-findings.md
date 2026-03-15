# Fix all adversarial review findings in shedul3r-rs-sdk and pipelin3r

**Date:** 2026-03-15 13:43
**Task:** Fix P-C1, P-C2, P-C3, P-H1, P-H2, P-H3, P-H4, P-H5 findings

## Goal
All critical and high-priority adversarial review findings resolved. Both packages compile, pass tests, and pass strict clippy.

## Approach

### Step-by-step plan

1. **P-H3: Replace anyhow with thiserror** (do first since it touches everything)
   - Create `SdkError` in SDK `src/error.rs`
   - Create `PipelineError` in pipelin3r `src/error.rs`
   - Replace all `anyhow::Result` with typed errors across both packages
   - Keep anyhow as dev-dependency for test functions

2. **P-C2: poll_for_file timeout** — Add `max_poll_duration` to `ClientConfig`, track elapsed in `poll_for_file`, return `SdkError::PollTimeout`

3. **P-C1: Auth::FromEnv error** — Change `to_env()` to return `Result<BTreeMap>`, error if no credentials found

4. **P-C3: Template single-pass** — Replace sequential content replacement with reverse-position single-pass, update test

5. **P-H1: Bundle cleanup on failure** — Wrap remote sections in agent.rs to always clean up

6. **P-H2: Explicit tokio features** — Replace `features = ["full"]`

7. **P-H4: BTreeMap everywhere** — Replace all HashMap usage

8. **P-H5: tempfile::tempdir** — Replace PID-based dir with `tempfile::tempdir()`, return `TempDir`

## Files to Modify
- `packages/shedul3r-rs-sdk/Cargo.toml` — add thiserror, serde_json; remove anyhow (keep as dev-dep)
- `packages/shedul3r-rs-sdk/src/lib.rs` — export error module
- `packages/shedul3r-rs-sdk/src/error.rs` — new: SdkError enum
- `packages/shedul3r-rs-sdk/src/client.rs` — typed errors, poll timeout, BTreeMap
- `packages/shedul3r-rs-sdk/src/bundle.rs` — typed errors
- `packages/pipelin3r/Cargo.toml` — add thiserror, tempfile to deps; explicit tokio features; remove anyhow (keep as dev-dep)
- `packages/pipelin3r/src/lib.rs` — export error module
- `packages/pipelin3r/src/error.rs` — new: PipelineError enum
- `packages/pipelin3r/src/auth.rs` — BTreeMap, Result return, error on empty FromEnv
- `packages/pipelin3r/src/agent.rs` — BTreeMap, typed errors, bundle cleanup
- `packages/pipelin3r/src/bundle.rs` — tempfile::tempdir, typed errors
- `packages/pipelin3r/src/template.rs` — single-pass content replacement
- `packages/pipelin3r/src/command.rs` — typed errors
- `packages/pipelin3r/src/transform.rs` — typed errors
- `packages/pipelin3r/src/executor.rs` — typed errors
- `packages/pipelin3r/src/task.rs` — typed errors
- `packages/pipelin3r/src/pool.rs` — typed errors
- `packages/pipelin3r/src/model.rs` — typed errors
