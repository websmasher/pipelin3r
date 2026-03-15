# Fix 5 adversarial re-review issues

**Date:** 2026-03-15 17:30
**Task:** Fix 5 remaining issues from adversarial re-review

## Goal
All 5 issues resolved, all tests pass in both workspaces, golden tests pass.

## Approach

### 1. URL-encoding breaks nested file paths (HIGH)
**File:** `packages/shedul3r-rs-sdk/src/bundle.rs`
- Change `urlencoding::encode(path)` to per-segment encoding in `download_file()`
- Same defensive change for `bundle_id` in `download_file()` and `delete_bundle()`
- Update the existing regression test which currently accepts `%2F` — it should now reject it
- Add new test: `download_file("id", "sub/dir/file.txt")` URL must have literal `/`

### 2. Auth middleware blocks /health (MEDIUM)
**File:** `apps/shedul3r/crates/adapters/inbound/api/src/main.rs`
- Split routes into public (health) and protected (task, bundle)
- Apply auth middleware only to protected routes
- Add test verifying health endpoint works without auth when key is set

### 3. limiter_status() stubbed (MEDIUM)
**File:** `apps/shedul3r/crates/app/commands/src/execute.rs`
- Add doc comment explaining intentional deferral
- No code change beyond the comment

### 4. working_directory validation (MEDIUM)
**File:** `apps/shedul3r/crates/app/commands/src/execute.rs`
- Before building `SubprocessCommand`, validate `working_directory`:
  - Must be absolute path
  - Must not contain `..` components
  - Must exist and be a directory
- Return `SchedulrError::TaskDefinition` on violation

### 5. Circuit breaker YAML config (MEDIUM)
**File:** `apps/shedul3r/crates/app/commands/src/parser.rs`
- Add `parse_circuit_breaker()` following `parse_rate_limit()`/`parse_retry()` pattern
- YAML keys: `circuit-breaker.failure-rate-threshold`, `circuit-breaker.sliding-window-size`, `circuit-breaker.wait-duration-in-open-state`
- Add `circuit_breaker_config: Option<CircuitBreakerConfig>` to `TaskDefinition`
- Wire parsed config into `execute.rs` instead of using `default_circuit_breaker_config()`
- Add parser tests

## Files to Modify
- `packages/shedul3r-rs-sdk/src/bundle.rs` — per-segment URL encoding
- `apps/shedul3r/crates/adapters/inbound/api/src/main.rs` — split routes for auth
- `apps/shedul3r/crates/app/commands/src/execute.rs` — limiter_status comment, working_directory validation, wire CB config
- `apps/shedul3r/crates/app/commands/src/parser.rs` — parse circuit-breaker YAML
- `apps/shedul3r/crates/domain/types/src/task.rs` — add circuit_breaker_config field
