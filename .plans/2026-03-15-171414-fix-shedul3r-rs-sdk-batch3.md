# Fix shedul3r-rs-sdk adversarial review findings (Batch 3)

**Date:** 2026-03-15 17:14
**Task:** Fix 6 issues from adversarial review v2 Batch 3

## Goal
Make shedul3r-rs-sdk properly propagate errors, URL-encode paths, add missing fields, and clean up error variants.

## Approach

### 1. http_call never returns Err
- Rewrite http_call so network/parse errors propagate via `?` (with file recovery check before propagating)
- Only task-level failures (success!=true) use Ok(TaskResult{success:false})
- submit_task_with_recovery poll_failed branch: propagate Err after file check

### 2. Bundle path not URL-encoded
- Add `urlencoding = "2"` to Cargo.toml
- URL-encode bundle_id and path in download_file and delete_bundle

### 3. File-poll recovery tests
- Add tokio test: spawn task that creates file after 100ms, poll with 500ms max
- Add test for submit_task_with_recovery recovery path concept (file appears during poll)

### 4. Missing TaskPayload fields
- Add `limiter_key: Option<String>` and `timeout_ms: Option<u64>` to TaskPayload
- Update lib.rs doc example

### 5. Response metadata in TaskResult
- Add exit_code, elapsed, started_at optional fields to TaskResult
- Add ApiResponseMetadata struct for deserialization
- Update ApiResponse with metadata field
- Wire through in http_call

### 6. Clean up error variants
- Add `require_success()` method to TaskResult using SdkError::TaskFailed
- SdkError::Json is used via `#[from]` so keep it (reqwest json parsing could use it)

## Files to Modify
- `packages/shedul3r-rs-sdk/Cargo.toml` — add urlencoding dep
- `packages/shedul3r-rs-sdk/src/client.rs` — fix http_call, add fields, add tests
- `packages/shedul3r-rs-sdk/src/bundle.rs` — URL-encode paths
- `packages/shedul3r-rs-sdk/src/error.rs` — keep variants, they're all wired
- `packages/shedul3r-rs-sdk/src/lib.rs` — update doc example with new fields
