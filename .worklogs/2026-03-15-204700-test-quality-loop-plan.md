# Plan: Automated test quality loop

**Date:** 2026-03-15 20:47
**Scope:** Plans only

## Summary
Wrote plan for closed-loop automated test quality: fuzz → case reduce → add to suite → mutation test → fill gaps → suite reduce → check convergence → repeat. The missing piece is `cargo-test-reduce` (Rust test suite reduction via per-test coverage + set cover). Everything else exists as standalone tools.
