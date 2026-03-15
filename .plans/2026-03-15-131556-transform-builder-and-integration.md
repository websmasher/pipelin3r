# Implement TransformBuilder and integration tests

**Date:** 2026-03-15 13:15
**Task:** Implement transform builder (pure Rust function step) and integration tests for the full agent flow.

## Goal
1. Replace the TransformBuilder stub with a working implementation that reads files, applies a transform function, and writes outputs.
2. Add integration tests for agent dry-run (single + batch) and transform builder.

## Approach

### Step 1: Implement TransformBuilder in transform.rs
- Add fields: `input_files: Vec<PathBuf>`, `transform_fn: Option<Box<dyn FnOnce(...)>>`
- Builder methods: `input_file`, `input_files`, `apply`
- `execute` method: read files, call transform, write outputs, return `TransformResult`
- Must satisfy strict clippy: no unwrap, no indexing, no arithmetic overflow

### Step 2: Add unit tests to transform.rs
- Filter test: read 3 files, output 2
- Uppercase test: modify content
- Empty inputs test

### Step 3: Create integration test file
- `packages/pipelin3r/tests/integration.rs`
- Agent dry-run single: verify capture directory files
- Agent dry-run batch: verify 3 capture directories
- Use tempdir for isolation

### Key decisions
- **TransformResult as simple struct:** Matches CommandResult/AgentResult pattern — public fields, no methods needed yet.
- **FnOnce for transform_fn:** Transform runs once per execute call, no need for Fn/FnMut.
- **Export TransformResult from lib.rs:** Following the pattern of exporting AgentResult, CommandResult.

## Files to Modify
- `packages/pipelin3r/src/transform.rs` — full implementation
- `packages/pipelin3r/src/lib.rs` — add TransformResult to exports
- `packages/pipelin3r/tests/integration.rs` — new file for integration tests
- `packages/pipelin3r/Cargo.toml` — add tempfile dev-dependency
