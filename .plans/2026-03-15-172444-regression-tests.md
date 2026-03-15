# Add regression tests for limit3r and shedul3r bugs

**Date:** 2026-03-15 17:24
**Task:** Add regression tests that would catch 12 specific bugs that were previously fixed.

## Goal
Each regression test should fail if the corresponding bug is re-introduced. Tests go in existing `#[cfg(test)] mod tests` blocks.

## Approach

### limit3r tests (bugs 1-7)
Most already exist! After reading the code:
- Bug 1 (premature trip on partial window): Already covered by `single_failure_does_not_trip_with_large_window` — but need to add the specific scenario from the spec (window=100, 1 failure)
- Bug 2 (>= threshold): Already covered by `trips_at_exact_threshold_rate` — but uses window=4. Add window=2 variant.
- Bug 3 (rate limiter eviction): Need to add test in rate_limiter.rs
- Bug 4 (circuit breaker eviction drops failure history): Need to add test in circuit_breaker.rs
- Bug 5 (bulkhead eviction): Need to add test in bulkhead.rs
- Bug 6 (config validation): Already covered by existing tests. Skip duplicates.
- Bug 7 (duration_serde negative/NaN): Already covered by existing tests. Skip duplicates.

### shedul3r tests (bugs 8-12)
- Bug 8 (architecture test): Test in db crate that reads source and asserts no `impl RateLimiter for`
- Bug 9 (path traversal): Already covered by existing tests in bundles.rs. Skip.
- Bug 10 (body size limit): Need to add test in bundles.rs
- Bug 11 (exit code): Need to add test in subprocess crate
- Bug 12 (auth): Need to add test in api crate's auth module or handlers

## Files to Modify
- `packages/limit3r/src/circuit_breaker.rs` — bugs 1, 2, 4
- `packages/limit3r/src/rate_limiter.rs` — bug 3
- `packages/limit3r/src/bulkhead.rs` — bug 5
- `packages/limit3r/src/config.rs` — bug 6 (already covered, add explicit regression-named tests)
- `packages/limit3r/src/duration_serde.rs` — bug 7 (already covered, add explicit regression-named tests)
- `apps/shedul3r/crates/adapters/outbound/db/src/lib.rs` — bug 8
- `apps/shedul3r/crates/adapters/inbound/api/src/handlers/bundles.rs` — bugs 9, 10
- `apps/shedul3r/crates/adapters/outbound/subprocess/src/lib.rs` — bug 11
- `apps/shedul3r/crates/adapters/inbound/api/src/auth.rs` — bug 12
