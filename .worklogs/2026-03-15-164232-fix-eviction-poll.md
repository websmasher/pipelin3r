# Fix eviction self-eviction + circuit breaker history loss + poll overshoot

**Date:** 2026-03-15 16:42
**Scope:** limit3r eviction, SDK poll timing

## Fixes
- Rate limiter: evict after current key inserted, exclude current key from eviction
- Bulkhead: same — evict after, exclude current
- Circuit breaker: two-pass eviction (empty-closed first, then fewest-results-closed), preserve failure history, exclude current key
- Poll: sleep min(interval, remaining) to prevent overshoot
