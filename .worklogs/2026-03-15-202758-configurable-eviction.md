# Configurable MAX_TRACKED_KEYS, limit3r 84→87%

**Date:** 2026-03-15 20:27
**Scope:** limit3r — configurable eviction threshold

## Summary
Made MAX_TRACKED_KEYS configurable via with_max_keys() constructor on all InMemory* implementations. Rewrote eviction tests to use max_keys=5 instead of 10K entries. Tests are faster and more precise. Kill rate 84→87%.

## Final kill rates
- limit3r: 87% (14 survivors — boundary `>` vs `>=` precision)
- SDK: 95% (1 survivor)
- pipelin3r: 100% (0 survivors)
