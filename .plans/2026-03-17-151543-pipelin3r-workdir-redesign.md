# pipelin3r WorkDir Redesign — Bundle → Path-Based Composition

**Date:** 2026-03-17 15:15
**Task:** Redesign pipelin3r's block interface around work directories (filesystem paths) instead of in-memory bundles, making blocks composable via standard filesystem operations.

## Goal

After this work, pipelin3r blocks operate on `(prompt, work_dir_path) → (response, work_dir_path)`. Users assemble input directories with `std::fs`, pass a `&Path` to the block, and read output files with `std::fs`. The `Bundle` type disappears from the public API. Local vs remote transport is auto-detected from the shedul3r URL.

## Context

### Why this redesign

Both real-world pipelines built on this stack (websmasher/dev-process 26 steps, steady-parent 18 steps) follow the same pattern:

1. Assemble a directory with input files
2. Send a prompt + that directory to Claude Code via shedul3r
3. Agent reads/writes files in that directory
4. Next step reads from that directory

The current API forces users to construct `Bundle` objects (in-memory file collections), set `working_dir`, set `expected_output`, and manage remote upload/download via a `.remote()` flag — all separate concepts for what is fundamentally "here's a folder, run the agent in it."

### Design decisions from discussion

1. **A block has two channels**: message (prompt/response) and workspace (input folder/output folder)
2. **The workspace is just a `PathBuf`** — no wrapper type. Users use `std::fs` for assembly and extraction.
3. **Transport is auto-detected**: if shedul3r URL is localhost → pass path directly. Otherwise → upload via bundle endpoints, download after execution.
4. **`Bundle` becomes an internal transport mechanism**, not a public API concept.

## Approach

### Step 1: Executor — remove `.remote()`, add URL-based auto-detection

**File:** `packages/pipelin3r/src/executor/mod.rs`

- Remove `remote: bool` field from `Executor`
- Remove `with_remote(self) -> Self` method
- Remove `is_remote()` method
- Add `fn is_local(&self) -> bool` (private) that checks if the SDK client's base URL is `localhost` or `127.0.0.1`
- The executor's `new()` already takes a URL via `ClientConfig` — no new config needed

### Step 2: AgentBuilder — replace bundle/working_dir/expected_output with work_dir

**File:** `packages/pipelin3r/src/agent/mod.rs`

Current fields to remove from `AgentBuilder`:
- `working_dir: Option<PathBuf>`
- `expected_output: Option<PathBuf>`
- `bundle_data: Option<Bundle>`

Replace with:
- `work_dir: Option<PathBuf>` — the single concept

Current fields to remove from `AgentTask` (batch):
- `working_dir: Option<PathBuf>`
- `expected_output: Option<PathBuf>`
- `bundle_data: Option<Bundle>`

Replace with:
- `work_dir: Option<PathBuf>`

Remove these builder methods:
- `.working_dir()`
- `.expected_output()`
- `.bundle()`

Add:
- `.work_dir(path: &Path) -> Self`

### Step 3: Execution logic — auto-transport in execute

**File:** `packages/pipelin3r/src/agent/execute.rs`

Rewrite `execute_remote_bundle` → `execute_with_work_dir`:

**Local path (shedul3r on localhost):**
1. Pass `work_dir` path as `working_directory` in `TaskPayload`
2. Submit task
3. Return result — output files are already in `work_dir`

**Remote path (shedul3r on another machine):**
1. Read all files from `work_dir` into memory
2. Upload as bundle via SDK's `upload_bundle()`
3. Pass bundle's remote path as `working_directory` in `TaskPayload`
4. Submit task
5. Download all files from bundle back to local `work_dir` (replaces the `expected_output`-based selective download)
6. Delete remote bundle
7. Return result

The key change: instead of downloading only `expected_output` files, download **everything** from the remote bundle back. The user's work_dir should look the same after execution regardless of local/remote.

Note: "download everything" requires a new SDK method or a list endpoint on shedul3r's bundle API. Current API only supports `GET /api/bundles/{id}/files/{path}` for individual files. Options:
- Add `GET /api/bundles/{id}/files` (list all files) to shedul3r
- Or add `GET /api/bundles/{id}/archive` (download as tar/zip) to shedul3r
- Or keep the `expected_output` concept internally but derive it from directory diffing

**Decision: defer the "download everything" problem.** For now, keep an optional `.expect_outputs(&["file1", "file2"])` method on the builder for remote mode. This is a transport hint, not a workspace concept — it tells the executor which files to pull back. If not specified and remote, warn or error. This can be improved later with a bundle listing endpoint.

### Step 4: Batch execution — work_dir per task

**File:** `packages/pipelin3r/src/agent/mod.rs`

`AgentBatchBuilder` changes:
- `for_each` closure returns `AgentTask` which now only has `.prompt()`, `.work_dir()`, `.auth()`
- Each task in the batch gets its own `work_dir`
- Execution logic applies the same local/remote transport per task

### Step 5: CommandBuilder — same pattern

**File:** `packages/pipelin3r/src/command/mod.rs`

- Already has no bundle concept
- Already takes a working directory
- Rename `.working_dir()` to `.work_dir()` for consistency
- No other changes needed

### Step 6: Remove Bundle from public API

**File:** `packages/pipelin3r/src/lib.rs`

- Remove `pub use bundle::Bundle` from public re-exports
- Change `bundle` module visibility to `pub(crate)`
- `Bundle` struct stays internally for remote transport serialization
- Or: replace internal `Bundle` usage with direct `std::fs::read_dir` → upload, since we're reading from a directory path now, not constructing in-memory

### Step 7: Update dry-run capture

**File:** `packages/pipelin3r/src/agent/execute.rs`

`execute_dry_run_capture` currently records bundle file paths in meta.json. Update to:
- Record `work_dir` path instead
- List files in the work_dir directory for the meta capture
- Remove bundle-specific logic

### Step 8: Update integration tests

**File:** `packages/pipelin3r/tests/integration.rs`

- Rewrite tests to use `tempdir` + `std::fs::write` instead of `Bundle::new().add_text_file()`
- Test local execution with work_dir
- Test that output files appear in work_dir after execution

### Step 9: Update TransformBuilder

**File:** `packages/pipelin3r/src/transform/mod.rs`

- Already file-path based (`.input_file()`)
- Consider aligning to take a `work_dir` for consistency, or leave as-is since transforms don't go through shedul3r

## What stays unchanged

- `TemplateFiller` — prompt assembly is orthogonal to workspace
- `Auth` — auth injection is orthogonal
- `Model`, `Provider`, `Tool` — model selection is orthogonal
- `run_pool` — concurrency primitive, unchanged
- `task` module — YAML builder, unchanged
- `shedul3r-rs-sdk` — bundle upload/download methods stay, just used internally now
- `error` module — may need minor updates to remove `Bundle`-specific variants

## Public API after redesign

```rust
// Re-exports from lib.rs
pub use agent::{AgentBuilder, AgentResult, AgentTask};
pub use auth::Auth;
// Bundle REMOVED from public API
pub use command::{CommandBuilder, CommandResult};
pub use error::PipelineError;
pub use executor::Executor;
pub use model::{Model, ModelConfig, Provider, Tool};
pub use pool::run_pool;
pub use template::TemplateFiller;
pub use transform::{TransformBuilder, TransformResult};
```

## Risks & Edge Cases

1. **"Download everything" for remote mode** — current bundle API only supports individual file downloads. Deferred: use `.expect_outputs()` hint for now.
2. **Large work directories** — uploading a 500MB directory as a bundle may be slow. This is an existing problem with bundles, not new.
3. **Temp directory lifecycle** — `tempfile::TempDir` drops and deletes on scope exit. Users must keep the `TempDir` alive as long as they need the output files. This is standard Rust — not a pipelin3r concern.
4. **Breaking change** — this removes `Bundle` from the public API. Since the crate is pre-1.0 and unpublished, this is acceptable.
5. **Work dir without prompt** — some blocks might only need a work_dir (transform) or only a prompt (simple query). Both should remain optional.

## Files to Modify

- `packages/pipelin3r/src/executor/mod.rs` — remove remote flag, add is_local detection
- `packages/pipelin3r/src/agent/mod.rs` — replace bundle/working_dir/expected_output with work_dir
- `packages/pipelin3r/src/agent/execute.rs` — rewrite transport logic
- `packages/pipelin3r/src/command/mod.rs` — rename working_dir to work_dir
- `packages/pipelin3r/src/bundle/mod.rs` — make pub(crate) or refactor for path-based reading
- `packages/pipelin3r/src/lib.rs` — remove Bundle from public exports
- `packages/pipelin3r/src/error.rs` — remove/rename Bundle error variant if needed
- `packages/pipelin3r/tests/integration.rs` — rewrite tests
