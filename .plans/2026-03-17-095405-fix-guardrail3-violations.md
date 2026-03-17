# Fix guardrail3 violations in shedul3r app crates

**Date:** 2026-03-17 09:54
**Task:** Fix R44, R38, R-TEST-09, R-PUB-08 violations

## Goal
All guardrail3 checks pass for shedul3r app crates.

## Approach

### R44 — unwrap() in production code
- `main.rs` lines 74,76: `unwrap_or` on port parse — already has fallback, not actually unwrap violations
- `main.rs` lines 39,144,147: `expect_used` — already have `#[allow]` with justification
- `subprocess/src/lib.rs` line 196: `unwrap_or(-1)` — not an unwrap violation, it's unwrap_or
- `subprocess/src/lib.rs` tests: already have `#[allow(clippy::unwrap_used)]`
- `auth.rs` test: already has allow
- `cli.rs` line 118: `unwrap_or_else` — not an unwrap violation
- `db/src/lib.rs` test line 29: already has allow at module level

After review: the unwraps flagged are either already allowed or are `unwrap_or`/`unwrap_or_else` which are NOT unwrap violations. No changes needed for R44.

### R38 — File too long: execute.rs (709 lines)
- Extract `#[cfg(test)] mod tests` into `src/execute/tests.rs`
- Convert `src/execute.rs` to `src/execute/mod.rs`

### R-TEST-09 — Extract test modules into separate files
7 files need test extraction:
1. `commands/src/execute.rs` -> `commands/src/execute/mod.rs` + `tests.rs`
2. `commands/src/parser.rs` -> `commands/src/parser/mod.rs` + `tests.rs`
3. `rest/src/auth.rs` -> `rest/src/auth/mod.rs` + `tests.rs`
4. `rest/src/handlers/bundles.rs` -> `rest/src/handlers/bundles/mod.rs` + `tests.rs`
5. `rest/src/cli.rs` -> `rest/src/cli/mod.rs` + `tests.rs`
6. `subprocess/src/lib.rs` -> keep lib.rs, create `src/tests.rs`
7. `db/src/lib.rs` -> keep lib.rs, create `src/tests.rs`

### R-PUB-08 — Already resolved
All crates use `version.workspace = true` which resolves to `0.3.0`.

## Files to Modify
- All 7 files listed above for test extraction
