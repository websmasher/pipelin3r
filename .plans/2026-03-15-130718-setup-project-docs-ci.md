# Set up CLAUDE.md, deny.toml, and CI for pipelin3r monorepo

**Date:** 2026-03-15 13:07
**Task:** Create root CLAUDE.md, root deny.toml, and GitHub Actions CI workflow

## Goal
Three files: CLAUDE.md (project docs for agents), deny.toml (dependency policy for packages workspace), ci.yml (GitHub Actions CI).

## Approach

### Step-by-step plan
1. Write `/CLAUDE.md` — document architecture, packages, build/test commands, don'ts
2. Write `/deny.toml` — based on shedul3r's deny.toml, adapted for packages workspace (no axum/sqlx/web-framework bans since packages are libraries)
3. Write `/.github/workflows/ci.yml` — two jobs (packages + shedul3r) plus golden tests
4. Verify `cargo deny check` passes from root

## Files to Modify
- `CLAUDE.md` — new file, project documentation
- `deny.toml` — new file, cargo-deny config for packages workspace
- `.github/workflows/ci.yml` — new file, CI pipeline
