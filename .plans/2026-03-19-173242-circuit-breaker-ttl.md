# Fix circuit breaker: add TTL to sliding window entries

**Date:** 2026-03-19 17:32
**Task:** Old failures in the sliding window never expire, permanently poisoning keys

## Bug
The sliding window is count-based (VecDeque<bool>). Entries have no timestamps. Failures from hours ago still count toward the failure rate. A key that had 5 failures last run will trip immediately on the next run even though the underlying issue may be resolved.

## Fix
Add timestamps to window entries. When calculating failure rate, ignore entries older than `wait_duration_in_open_state * 2` (use the existing config value as a TTL proxy — if we're willing to wait 30s in open state, entries older than 60s are stale).

## Approach
1. Change `results: VecDeque<bool>` to `results: VecDeque<(Instant, bool)>`
2. In `trim_to_window`, also evict entries older than TTL
3. In `failure_rate`, only count non-expired entries
4. Update tests

## Files
- `packages/limit3r/src/circuit_breaker/mod.rs`
- `packages/limit3r/src/circuit_breaker/tests.rs`
