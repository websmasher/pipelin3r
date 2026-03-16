# Fix eviction logic in limit3r + poll overshoot in shedul3r-rs-sdk

**Date:** 2026-03-15 16:38
**Task:** Fix 4 bugs: rate limiter eviction can evict current key, bulkhead eviction can evict current key, circuit breaker eviction drops all Closed keys losing failure history, poll_for_file can overshoot max_duration.

## Goal
Eviction in all three limit3r components must never evict the key currently being operated on, and circuit breaker eviction must preserve failure history. Poll function must not overshoot timeout.

## Approach

### Step-by-step plan
1. **rate_limiter.rs** — Move eviction block after the key insert/lookup. Exclude current key from both retain and oldest-eviction passes.
2. **bulkhead.rs** — Move eviction block after key insert/lookup. Exclude current key from retain.
3. **circuit_breaker.rs** — Replace blanket `state != Closed` eviction with two-pass: first evict closed+empty-results, then evict closed with fewest results. Always exclude current key.
4. **client.rs** — In poll_for_file loop, sleep min(interval, remaining) instead of full interval.

## Files to Modify
- `packages/limit3r/src/rate_limiter.rs` — eviction after lookup, exclude current key
- `packages/limit3r/src/bulkhead.rs` — eviction after lookup, exclude current key
- `packages/limit3r/src/circuit_breaker.rs` — two-pass eviction preserving failure history
- `packages/shedul3r-rs-sdk/src/client.rs` — poll overshoot fix
