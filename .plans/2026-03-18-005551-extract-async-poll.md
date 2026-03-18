# Extract async polling code from client/mod.rs

**Date:** 2026-03-18 00:55
**Task:** Split client/mod.rs (502 lines) by extracting async polling code into client/async_poll.rs

## Goal
Both files under 500 effective lines. No behavior changes.

## Approach
1. Create `async_poll.rs` with: `AsyncSubmitResponse`, `AsyncTaskStatus` (struct+impl), `api_response_to_result` fn, and three `impl Client` methods (`submit_task_async`, `get_task_status`, `submit_task_poll`)
2. In `mod.rs`: add `mod async_poll;`, re-export `AsyncTaskStatus`, remove extracted items
3. Ensure `ApiResponse`, `ApiResponseMetadata`, `ApiElapsed` remain `pub(crate)` (already are)
4. Verify with `cargo test -p shedul3r-rs-sdk`

## Files to Modify
- `packages/shedul3r-rs-sdk/src/client/mod.rs` — remove extracted items, add module decl + re-export
- `packages/shedul3r-rs-sdk/src/client/async_poll.rs` — new file with extracted code
