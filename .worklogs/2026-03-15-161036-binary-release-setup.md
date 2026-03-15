# Set up shedul3r binary release via production branch

**Date:** 2026-03-15 16:10
**Scope:** Binary release workflow, stub crate, production branch

## Summary
Switch binary release trigger from main+path to production branch push (matching steady-parent pattern). Create stub crate for cargo binstall. Create production branch.

## Release flow
1. Develop on main, CI runs tests
2. PR from main → production
3. Merge triggers release-binary.yml
4. CI builds linux-x86_64 + darwin-arm64, creates GitHub Release
5. `cargo binstall shedul3r` downloads from the release
