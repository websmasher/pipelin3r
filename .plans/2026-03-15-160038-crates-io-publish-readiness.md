# Make all three packages publishable to crates.io

**Date:** 2026-03-15 16:00
**Task:** Fix LICENSE, dependency version, READMEs, CI publish check, Cargo.toml excludes, and binary release workflow

## Goal
All three packages (limit3r, shedul3r-rs-sdk, pipelin3r) pass `cargo publish --dry-run`.

## Approach

### Step-by-step plan
1. Create MIT LICENSE at repo root
2. Add `version = "0.1.0"` to shedul3r-rs-sdk dependency in pipelin3r/Cargo.toml
3. Write proper READMEs for all three packages based on actual source code
4. Add `exclude` to each package's Cargo.toml
5. Add `license-file` to each Cargo.toml pointing to root LICENSE (since license files must be included in published crate)
6. Add publish-check job to CI
7. Create binary release workflow for shedul3r

## Files to Modify
- `LICENSE` — create MIT license
- `packages/pipelin3r/Cargo.toml` — add version to shedul3r-rs-sdk dep, add exclude
- `packages/limit3r/Cargo.toml` — add exclude
- `packages/shedul3r-rs-sdk/Cargo.toml` — add exclude
- `packages/limit3r/README.md` — rewrite with real content
- `packages/shedul3r-rs-sdk/README.md` — rewrite with real content
- `packages/pipelin3r/README.md` — rewrite with real content
- `.github/workflows/ci.yml` — add publish-check job
- `.github/workflows/release-binary.yml` — create new workflow
