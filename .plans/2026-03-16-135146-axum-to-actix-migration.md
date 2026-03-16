# Migrate shedul3r API from Axum to actix-web + rename to /rest

**Date:** 2026-03-16 13:51
**Task:** Rename the `api` inbound adapter crate to `rest`, migrate from Axum to actix-web. Prerequisite for adding an MCP transport crate.

## Goal

The `crates/adapters/inbound/api/` crate becomes `crates/adapters/inbound/rest/` using actix-web instead of Axum. All existing functionality preserved: task execution, bundle upload/download, auth middleware, CLI mode, health endpoint. All tests passing.

## Approach

### Phase 1: Rename crate directory and references
1. Rename `crates/adapters/inbound/api/` → `crates/adapters/inbound/rest/`
2. Update `Cargo.toml` workspace members
3. Update all `use api::` imports to `use rest::`
4. Update crate name in `crates/adapters/inbound/rest/Cargo.toml`

### Phase 2: Replace dependencies
1. Remove: `axum`, `tower-http`
2. Add: `actix-web` (4.x), `actix-multipart`, `actix-rt`
3. Keep: tokio, serde, tracing, clap, parking_lot, uuid, domain-types, commands, subprocess, db

### Phase 3: Migrate files (9 files need changes)
1. **error.rs** — `IntoResponse` → `ResponseError` (smallest, no deps on other files)
2. **extractors.rs** — Rewrite `ValidatedJson` with actix `FromRequest`
3. **auth.rs** — Rewrite middleware from tower to actix wrap_fn
4. **handlers/tasks.rs** — Routes + handlers + tests
5. **handlers/bundles.rs** — Routes + multipart + streaming + tests
6. **handlers/mod.rs** — Update router return types
7. **main.rs** — Server bootstrap, middleware wiring
8. **lib.rs** — Update exports

### Phase 4: Test
1. `cargo build --release` in apps/shedul3r/
2. `cargo test --workspace` in apps/shedul3r/
3. Golden tests: `bash golden-tests/compare.sh`
4. Deploy to Railway, hit /health

## Key Decisions

- **actix-web 4.x** — stable, mature, well-documented
- **No actix-web macros** for route handlers — use programmatic routing for consistency with current style
- **Keep CLI mode unchanged** — it doesn't touch the HTTP framework
- **state.rs stays the same** — AppState is framework-agnostic

## Files to Modify

- `apps/shedul3r/Cargo.toml` — workspace member rename
- `crates/adapters/inbound/rest/Cargo.toml` — deps swap
- `crates/adapters/inbound/rest/src/error.rs` — ResponseError trait
- `crates/adapters/inbound/rest/src/extractors.rs` — FromRequest rewrite
- `crates/adapters/inbound/rest/src/auth.rs` — middleware rewrite
- `crates/adapters/inbound/rest/src/handlers/tasks.rs` — routing + handlers
- `crates/adapters/inbound/rest/src/handlers/bundles.rs` — multipart + streaming
- `crates/adapters/inbound/rest/src/handlers/mod.rs` — return types
- `crates/adapters/inbound/rest/src/main.rs` — server bootstrap
- `crates/adapters/inbound/rest/src/lib.rs` — exports
