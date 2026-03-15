# Extract shedul3r-rs-sdk from dev-process HTTP client

**Date:** 2026-03-15 12:40
**Task:** Extract the HTTP client code from websmasher/tools/dev-process/src/schedulr.rs into the shedul3r-rs-sdk package.

## Goal
A working SDK crate with: Client struct, ClientConfig, TaskPayload, TaskResult, submit_task, submit_task_with_recovery (file-poll racing), truncate_str helper, and stub bundle endpoints. No pipeline-specific logic (dry-run, auth injection, step name extraction).

## Approach

### Step-by-step plan
1. Write `src/client.rs` — Client struct with ClientConfig builder, TaskPayload/TaskResult types, submit_task, submit_task_with_recovery, http_call, poll_for_file, truncate_str
2. Write `src/bundle.rs` — Stub bundle endpoints (upload/download/delete) returning anyhow::bail
3. Update `src/lib.rs` — Re-exports, remove unused dep suppressors
4. Verify with cargo check and cargo test

### Key decisions
- **ClientConfig with Default**: Makes timeouts configurable without breaking simple usage
- **No dry-run in SDK**: That's pipelin3r's concern, SDK just does HTTP
- **No auth injection**: Caller passes environment map, SDK doesn't know about CLAUDE_ACCOUNT
- **truncate_str as pub(crate)**: Internal helper, not part of public API

## Files to Modify
- `packages/shedul3r-rs-sdk/src/client.rs` — Full HTTP client implementation
- `packages/shedul3r-rs-sdk/src/bundle.rs` — Stub bundle endpoints
- `packages/shedul3r-rs-sdk/src/lib.rs` — Re-exports
