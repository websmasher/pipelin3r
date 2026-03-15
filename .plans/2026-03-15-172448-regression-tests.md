# Add regression tests for 17 fixed bugs

**Date:** 2026-03-15 17:24
**Task:** Add regression tests to shedul3r-rs-sdk and pipelin3r that catch re-introduced bugs.

## Goal
Each test fails if the corresponding bug is re-introduced. 17 tests total across SDK and pipelin3r.

## Approach

### SDK tests (in client.rs and bundle.rs)
1. http_call returns Err on network failure (already exists — verify)
2. Bundle path URL-encoding (verify %20 in URL construction)
3. Response metadata populated (exit_code, elapsed, started_at from mock)
4. Poll timeout overshoot (initial_delay > max_duration returns within bounds)
5. File-poll recovery (file appears after delay)
6. require_success on failed task (already exists — verify)
7. TaskPayload serializes limiter_key and timeout_ms

### pipelin3r tests (in bundle.rs, template.rs, auth.rs, agent.rs, model.rs, integration.rs)
8. Bundle path traversal rejection
9. Template phase-1 cross-injection prevention
10. Auth::FromEnv with no env vars returns Err
11. Template content-into-content injection prevention
12. AgentResult.require_success returns AgentFailed not Other
13. Tool enum as_str returns correct strings
14. TemplateFiller owned self chaining
15. Template::from_file with temp file
16. Dry-run auth capture
17. Dry-run bundle capture

## Files to Modify
- `packages/shedul3r-rs-sdk/src/client.rs` — SDK regression tests
- `packages/shedul3r-rs-sdk/src/bundle.rs` — URL-encoding test
- `packages/pipelin3r/src/bundle.rs` — path traversal regression
- `packages/pipelin3r/src/template.rs` — injection regression tests
- `packages/pipelin3r/src/auth.rs` — FromEnv regression
- `packages/pipelin3r/src/agent.rs` — AgentFailed regression
- `packages/pipelin3r/src/model.rs` — Tool enum regression
- `packages/pipelin3r/tests/integration.rs` — dry-run capture regressions
