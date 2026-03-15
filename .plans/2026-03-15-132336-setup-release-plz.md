# Set up release-plz for automated semver publishing

**Date:** 2026-03-15 13:23
**Task:** Add release-plz configuration and GitHub Actions workflow for automated crates.io publishing

## Goal
Three new files that enable release-plz to manage semver versioning and crates.io publishing for the three published packages (limit3r, shedul3r-rs-sdk, pipelin3r).

## Approach
1. Create `release-plz.toml` — declares which packages to publish
2. Create `cliff.toml` — git-cliff changelog config for conventional commits
3. Create `.github/workflows/release.yml` — GHA workflow running release-pr + release

## Files to Create
- `release-plz.toml`
- `cliff.toml`
- `.github/workflows/release.yml`
