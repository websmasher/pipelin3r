# Fix adversarial review findings in limit3r

**Date:** 2026-03-15 13:42
**Task:** Fix all adversarial review findings (L1, L3, L4, L5, L8, L9, L10, L13, L14)

## Goal
Address all identified issues in limit3r without breaking existing tests or API.

## Approach

### Step-by-step plan
1. **L1 (bulkhead docs):** Add doc comments to Bulkhead trait warning about acquire/release pairing
2. **L4 (unbounded memory):** Add MAX_TRACKED_KEYS eviction to rate_limiter, circuit_breaker, bulkhead
3. **L9 (retry error):** Add `last_message: String` to RetryExhausted, update retry executor and Display
4. **L3/L5/L8/L14 (config validation):** Add validate() methods to all config types
5. **L13 (duration_serde):** Guard against negative/NaN/infinite in deserialize
6. **L10 (docs):** Fix "sliding-window" to "fixed-window" in rate_limiter

## Files to Modify
- `src/traits.rs` — L1 doc comments
- `src/bulkhead.rs` — L4 eviction
- `src/circuit_breaker.rs` — L4 eviction
- `src/rate_limiter.rs` — L4 eviction, L10 doc fix
- `src/error.rs` — L9 RetryExhausted field
- `src/retry.rs` — L9 capture last error message
- `src/config.rs` — L3/L5/L8/L14 validate() methods
- `src/duration_serde.rs` — L13 negative/NaN guard
