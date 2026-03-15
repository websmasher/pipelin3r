# Regression tests + final re-review fixes

**Date:** 2026-03-15 17:34
**Scope:** All four crates — regression tests + remaining fixes

## Regression tests added (28 total)
- limit3r: 10 (circuit breaker partial window, eviction, config validation, duration serde, architecture check)
- SDK: 8 (http_call errors, metadata, poll timeout, file recovery, url encoding, require_success)
- pipelin3r: 10 (path traversal, template injection both phases, auth error, tool enum, dry-run capture, chaining)

## Final fixes from re-review
- SDK: URL-encode per-segment (not whole path) for nested files
- shedul3r: health endpoint exempted from auth middleware
- shedul3r: working_directory validation (absolute, no traversal, must exist)
- shedul3r: circuit breaker YAML config (parse from task definition)
- shedul3r: limiter_status documented as intentionally deferred

## 217 tests + 31 golden, all passing
