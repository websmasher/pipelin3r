# Add release-plz + TOML model config

**Date:** 2026-03-15 13:27
**Scope:** release-plz setup, model ID config

## Summary
Set up release-plz for automated semver publishing (release-plz.toml, cliff.toml, release.yml workflow). Replaced hardcoded model IDs with TOML config (models.toml embedded via include_str!, with fallback to hardcoded defaults). 11 new model config tests. Total: 106 tests.
