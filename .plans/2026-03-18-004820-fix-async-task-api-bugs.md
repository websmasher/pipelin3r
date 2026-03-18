# Fix async task API bugs found by adversarial testing

**Date:** 2026-03-18 00:48
**Task:** Fix all bugs in async task API (SDK + server)

## Goal
Fix 6 bugs across SDK client and server state management, update adversarial tests to verify correct behavior.

## Approach

### SDK fixes (client/mod.rs)
1. Fix path mismatch: `get_task_status` uses `/api/tasks/{id}` but server expects `/api/tasks/async/{id}`
2. Add `max_poll_duration` deadline to `submit_task_poll` loop, return `SdkError::PollTimeout`
3. Check HTTP status in `submit_task_async` before parsing JSON body
4. Check HTTP status in `get_task_status` for non-2xx/non-404 codes
5. Add transient error retry (up to 3 retries on 5xx) in `submit_task_poll`

### Server fixes (state.rs)
6. Change TTL to use `completed_at` instead of `created_at`
7. Log warning on `mark_completed`/`mark_failed` with unknown ID

### Test updates
- Update test 22 (`async_submit_500_with_task_id_body_accepted_as_success`) to expect Err
- Update test 1 (`poll_never_completes_does_not_run_forever`) to expect PollTimeout
- Update TTL test to reflect completed_at behavior

## Files to Modify
- `packages/shedul3r-rs-sdk/src/client/mod.rs` — SDK fixes 1-5
- `packages/shedul3r-rs-sdk/src/error.rs` — possibly no change needed (PollTimeout already exists)
- `packages/shedul3r-rs-sdk/src/client/tests/async_polling.rs` — update adversarial tests
- `apps/shedul3r/crates/adapters/inbound/rest/src/state.rs` — server fixes 6-7
- `apps/shedul3r/crates/adapters/inbound/rest/tests/adversarial_async_task.rs` — update TTL test
