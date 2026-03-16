# Kill 47 surviving mutants in limit3r

**Date:** 2026-03-15 19:37
**Task:** Write tests to kill all 47 surviving mutants across 5 files

## Goal
Add targeted tests to each file's existing `#[cfg(test)]` module that detect and fail for each surviving mutation.

## Approach

### duration_serde.rs (10 survivors)
Add `OptDuration` test struct with `#[serde(default, with = "crate::duration_serde::option")]`, then test:
- Deserialize Some value, verify correct Duration
- Deserialize null, verify None
- Deserialize negative, verify error
- Non-option path: test `||` vs `&&` by checking negative AND nan/infinity independently

### bulkhead.rs (8 survivors)
- Fill to exactly MAX_TRACKED_KEYS+1 to trigger eviction, verify map shrinks
- Fill to MAX_TRACKED_KEYS-1, verify no eviction
- Config change: acquire with max=2, then same key with max=5, verify new limit
- Eviction retains keys with outstanding permits

### circuit_breaker.rs (12 survivors)
- Fill to MAX_TRACKED_KEYS+1, trigger eviction
- Open/halfopen circuits survive eviction
- Two-pass eviction: closed-empty removed before closed-with-history
- trim_to_window: push 10 results, verify only window_size remain

### rate_limiter.rs (12 survivors)
- Fill to MAX_TRACKED_KEYS+1, trigger eviction
- Expired windows evicted before non-expired
- Oldest-first when no expired
- Deadline exceeded returns error
- Debug fmt test

### retry.rs (5 survivors)
- delay == max_delay returns max_delay
- NaN factor produces zero
- Negative delay produces zero

## Files to Modify
- `packages/limit3r/src/duration_serde.rs` — add option deserialize tests
- `packages/limit3r/src/bulkhead.rs` — add eviction + config change tests
- `packages/limit3r/src/circuit_breaker.rs` — add eviction + trim tests
- `packages/limit3r/src/rate_limiter.rs` — add eviction + deadline tests
- `packages/limit3r/src/retry.rs` — add boundary condition tests
