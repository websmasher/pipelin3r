# Add bundle upload/download/delete REST endpoints to shedul3r

**Date:** 2026-03-15 12:58
**Task:** Add three REST endpoints for bundle management: upload (POST), download file (GET), delete (DELETE)

## Goal
shedul3r gains bundle management endpoints allowing multipart file upload into temp directories, single-file download from bundles, and bundle cleanup.

## Approach

### Step-by-step plan
1. Add `tempfile` to workspace deps in root Cargo.toml and to api crate's Cargo.toml
2. Enable `multipart` feature on axum in workspace deps
3. Add `parking_lot` to api crate deps (for RwLock in BundleStore)
4. Add `BundleStore` to AppState in state.rs
5. Create handlers/bundles.rs with upload, download, delete handlers
6. Register module in handlers/mod.rs
7. Wire bundle_router into lib.rs and main.rs
8. Add `NotFound` variant to AppError for 404 responses
9. Add tests

### Key decisions
- **Use tokio::fs instead of std::fs**: clippy.toml bans all std::fs methods. tokio::fs is async-native and not banned.
- **Use parking_lot::RwLock**: std RwLock is banned by clippy.toml. parking_lot is already a workspace dep.
- **Store TempDir in BundleEntry**: dropping the entry auto-cleans the temp dir.
- **Use BTreeMap**: HashMap is banned by clippy.toml.

## Files to Modify
- `apps/shedul3r/Cargo.toml` — add tempfile workspace dep
- `apps/shedul3r/crates/adapters/inbound/api/Cargo.toml` — add tempfile, parking_lot deps
- `apps/shedul3r/crates/adapters/inbound/api/src/state.rs` — add BundleStore to AppState
- `apps/shedul3r/crates/adapters/inbound/api/src/handlers/bundles.rs` — new file with 3 handlers
- `apps/shedul3r/crates/adapters/inbound/api/src/handlers/mod.rs` — register bundles module
- `apps/shedul3r/crates/adapters/inbound/api/src/lib.rs` — export bundle_router
- `apps/shedul3r/crates/adapters/inbound/api/src/main.rs` — wire bundle_router into app
- `apps/shedul3r/crates/adapters/inbound/api/src/error.rs` — add NotFound variant
