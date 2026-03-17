# Fix ApiElapsed deserialization mismatch

**Date:** 2026-03-17 23:46
**Scope:** packages/shedul3r-rs-sdk/src/client/mod.rs

## Summary

Fixed the root cause of "error decoding response body" — the SDK expected `elapsed` as `{ secs, nanos }` struct but shedul3r serializes it as a float (e.g., `420.283`).

## Context & Problem

Every agent task failed with "error decoding response body" despite the agent succeeding and writing files. After adding raw byte capture to the SDK, the actual error was revealed:

```
invalid type: floating point `420.28365175`, expected struct ApiElapsed at line 1 column 35
```

shedul3r uses `duration_serde` from limit3r which serializes `Duration` as `as_secs_f64()` → a plain float. The SDK's `ApiElapsed` struct expected `{ secs: Option<u64>, nanos: Option<u32> }`. Every response failed JSON deserialization.

## Decision

Changed `ApiElapsed` from a struct to a `#[serde(untagged)]` enum accepting both formats:
- `Float(f64)` — current shedul3r format
- `Struct { secs, nanos }` — legacy format for backwards compatibility

## Information Sources

- Debug output from modified SDK showing exact JSON parse error
- limit3r/src/duration_serde/mod.rs — `as_secs_f64().serialize(serializer)`
- shedul3r domain-types/src/request.rs — `#[serde(with = "duration_serde")]`

## Key Files

- `packages/shedul3r-rs-sdk/src/client/mod.rs` — ApiElapsed enum + raw bytes parsing
