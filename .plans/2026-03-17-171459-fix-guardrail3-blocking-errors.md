# Fix guardrail3 blocking errors

**Date:** 2026-03-17 17:14
**Task:** Fix all `guardrail3 rs validate` blocking errors (✗) so pre-commit hook passes.

## Goal
Zero blocking errors from `guardrail3 rs validate`. Warnings (⚠) and info (ℹ) are acceptable.

## Errors to fix

### 1. [R26] Missing workspace lints
Add `warnings = "deny"` to `[workspace.lints.rust]` and `partial_pub_fields = "deny"` to `[workspace.lints.clippy]` in root Cargo.toml.

### 2. [R58] Direct std::fs calls (19 occurrences)
Source scan finds `std::fs::*` in production code. Functions already have `#[allow(clippy::disallowed_methods)]` but R58 is a separate AST scan. Two approaches:
- Add per-crate config with `profile = "library"` — R58 may only apply to `service` profile
- If R58 still fires after per-crate config, add `[rust.checks]` exclusion or check if guardrail3.toml supports R58 exceptions

### 3. [R-ARCH-04] No per-crate configuration
Add `[rust.crates.*]` sections in guardrail3.toml for each workspace member (packages only — apps/shedul3r has own workspace).

### 4. [R-GARDE-01] garde dependency missing
Add `garde = { version = "0.22", features = ["derive"] }` to workspace deps and reference from pipelin3r.

### 5. [R-TEST-09] Inline test code
- Extract tests from `packages/pipelin3r/src/bundle/mod.rs` to `tests.rs`
- Check `packages/shedul3r-rs-sdk/src/client/tests_regression.rs` — already a separate file but guardrail flagging it

## Files to Modify
- `Cargo.toml` — R26, R-GARDE-01
- `guardrail3.toml` — R-ARCH-04
- `packages/pipelin3r/Cargo.toml` — garde dep
- `packages/pipelin3r/src/bundle/mod.rs` — extract tests
- `packages/pipelin3r/src/bundle/tests.rs` — new file with extracted tests
- Various files for R58 if per-crate config doesn't resolve it
