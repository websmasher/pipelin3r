# Make MAX_TRACKED_KEYS configurable in limit3r

**Date:** 2026-03-15 20:14
**Task:** Make MAX_TRACKED_KEYS configurable via `with_max_keys()` constructor, fix boundary tests to use small values, remove expensive 10K tests.

## Goal
All three InMemory* structs accept configurable max_keys. Tests use small values (5-10) for precise boundary testing. Old 10K-entry tests removed.

## Approach

### Step-by-step plan
1. **rate_limiter.rs**: Add `max_tracked_keys: usize` field, `with_max_keys()` constructor, update `new()` to delegate, replace `MAX_TRACKED_KEYS` const usage with `self.max_tracked_keys`, rewrite eviction tests with small values.
2. **circuit_breaker.rs**: Same pattern — add field, constructor, update eviction logic, rewrite tests.
3. **bulkhead.rs**: Same pattern — add field, constructor, update eviction logic, rewrite tests.
4. Remove/replace all `mutant_kill_v2_*` and other tests that create 10K+ entries.

### Key decisions
- Keep `MAX_TRACKED_KEYS` const as the default value (10_000), used only in `new()`.
- Tests use `with_max_keys(5)` to test exact boundary: fill to 5 (no eviction), add 6th (eviction triggers).

## Files to Modify
- `packages/limit3r/src/rate_limiter.rs` — add field, constructor, update logic, rewrite tests
- `packages/limit3r/src/circuit_breaker.rs` — same
- `packages/limit3r/src/bulkhead.rs` — same
