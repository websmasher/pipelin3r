# Fix circuit breaker: add TTL to sliding window entries

**Date:** 2026-03-19 17:36
**Scope:** packages/limit3r/src/circuit_breaker/mod.rs, tests.rs

## Summary
The circuit breaker's sliding window was count-based with no time expiry. Failures from previous runs permanently poisoned keys. Added timestamps to window entries with a TTL derived from the open-state wait duration.

## Context & Problem
Pipeline step 5 (wrapper generation) failed with "Circuit breaker open for key 'claude'" on the very first call — before any step 5 call had been made. The breaker was poisoned by failures from a previous step 4 run (install commands that shared the key, or from a completely separate pipeline run hours earlier). The sliding window `VecDeque<bool>` had no timestamps, so old failures never aged out.

## Decisions Made
- **Chose:** TTL = `wait_duration_in_open_state * 4`. With the default 30s wait, entries older than 2 minutes are evicted.
- **Why:** The wait duration already represents "how long we consider a failure relevant." The multiplier gives a buffer so recent-but-not-current failures still count.
- **Alternatives:** Fixed TTL (e.g., 5 minutes) — rejected because it ignores the config. Time-based sliding window (replace count window entirely) — rejected because count-based windows are simpler and the TTL handles the staleness issue.

## Changes
- `results: VecDeque<bool>` → `results: VecDeque<(Instant, bool)>`
- `trim_to_window` evicts expired entries before trimming to count
- `record_outcome` stores `(Instant::now(), success)`
- New test: `stale_failures_expire_and_do_not_poison_window`
