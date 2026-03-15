# Fix limit3r bugs and eliminate shedul3r code duplication (Batch 1)

**Date:** 2026-03-15 17:13
**Task:** Fix circuit breaker premature trip, >= threshold bug, eliminate shedul3r duplicate implementations, add missing tests.

## Goal
Circuit breaker doesn't trip on insufficient data, >= threshold works correctly, shedul3r re-exports from limit3r instead of duplicating, comprehensive test coverage added.

## Approach

### Step-by-step plan
1. Fix circuit_breaker.rs: add min_calls check before evaluating failure rate
2. Fix circuit_breaker.rs: change `>` to `>=` for threshold comparison
3. Replace shedul3r db/src/lib.rs with re-exports from limit3r
4. Delete contents of shedul3r duplicate files (rate_limiter.rs, circuit_breaker.rs, bulkhead.rs, retry.rs)
5. Clean up db/Cargo.toml deps
6. Add tests for partial window, >= threshold, config validation, duration_serde, eviction

## Files to Modify
- `packages/limit3r/src/circuit_breaker.rs` — add min_calls, fix >= threshold
- `apps/shedul3r/crates/adapters/outbound/db/src/lib.rs` — re-export from limit3r
- `apps/shedul3r/crates/adapters/outbound/db/src/rate_limiter.rs` — delete contents
- `apps/shedul3r/crates/adapters/outbound/db/src/circuit_breaker.rs` — delete contents
- `apps/shedul3r/crates/adapters/outbound/db/src/bulkhead.rs` — delete contents
- `apps/shedul3r/crates/adapters/outbound/db/src/retry.rs` — delete contents
- `apps/shedul3r/crates/adapters/outbound/db/Cargo.toml` — remove unused deps
