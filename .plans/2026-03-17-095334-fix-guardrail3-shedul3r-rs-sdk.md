# Fix guardrail3 violations in shedul3r-rs-sdk

**Date:** 2026-03-17 09:53
**Task:** Fix R44, R58, R38, R-TEST-09 guardrail violations

## Goal
All guardrail3 checks pass for the shedul3r-rs-sdk package. Tests continue to pass.

## Approach

### Step-by-step plan
1. Convert `src/client.rs` → `src/client/mod.rs` + `src/client/tests.rs` (fixes R38 + R-TEST-09)
2. Convert `src/bundle.rs` → `src/bundle/mod.rs` + `src/bundle/tests.rs` (fixes R-TEST-09)
3. Add `#![allow(clippy::unwrap_used, reason = "test assertions")]` at top of each test module (fixes R44)
4. Remove individual `#[allow(clippy::unwrap_used)]` annotations that become redundant
5. Add `#[allow(clippy::disallowed_methods, reason = "test helper: std::fs used in spawned task")]` for std::fs calls in tests; for the production `std::fs::remove_file` at line 205, it's legitimate recovery cleanup — add justified allow on that line

## Files to Modify
- `src/client.rs` → split into `src/client/mod.rs` + `src/client/tests.rs`
- `src/bundle.rs` → split into `src/bundle/mod.rs` + `src/bundle/tests.rs`
