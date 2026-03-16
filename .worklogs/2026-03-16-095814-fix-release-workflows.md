# Fix release workflows

**Date:** 2026-03-16 09:58
**Scope:** .github/workflows/release.yml, .github/workflows/ci.yml

## Summary
Switch release trigger from main to production branch. Fix CI publish dry-run failure for packages with unpublished workspace dependencies.

## Context
Release was triggering on every push to main (agents push frequently). CI publish dry-run failed because pipelin3r depends on shedul3r-rs-sdk which isn't on crates.io yet.

## Decisions
- Release triggers on production branch only (agents push to main freely)
- pipelin3r dry-run allowed to fail with warning (resolves once shedul3r-rs-sdk is published)
