# Add limit3r unit tests + rename shedul3r binary

**Date:** 2026-03-15 12:58
**Task:** Add inline unit tests to limit3r source files; rename shedul3r binary from "schedulr" to "shedul3r"

## Goal
1. Each limit3r implementation file gets a `#[cfg(test)] mod tests` with specific test cases
2. The shedul3r binary name changes from "schedulr" to "shedul3r" in Cargo.toml and CLI command name

## Approach

### Task 1: limit3r tests
Add `#[cfg(test)] mod tests` to: rate_limiter.rs, circuit_breaker.rs, bulkhead.rs, retry.rs, config.rs
Each module gets `#[allow(clippy::unwrap_used, clippy::expect_used)]` with reason comment.
Tests use `#[tokio::test]` for async, `#[test]` for sync (config serde round-trip).

### Task 2: shedul3r binary rename
- `apps/shedul3r/crates/adapters/inbound/api/Cargo.toml`: `[[bin]] name = "schedulr"` -> `"shedul3r"`
- `apps/shedul3r/crates/adapters/inbound/api/src/cli.rs`: `#[command(name = "schedulr"` -> `"shedul3r"`

## Files to Modify
- `packages/limit3r/src/rate_limiter.rs` — add test module
- `packages/limit3r/src/circuit_breaker.rs` — add test module
- `packages/limit3r/src/bulkhead.rs` — add test module
- `packages/limit3r/src/retry.rs` — add test module
- `packages/limit3r/src/config.rs` — add test module
- `apps/shedul3r/crates/adapters/inbound/api/Cargo.toml` — rename binary
- `apps/shedul3r/crates/adapters/inbound/api/src/cli.rs` — rename CLI command
