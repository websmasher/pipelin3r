# Fix is_local() subdomain bypass security bug

**Date:** 2026-03-17 15:45
**Task:** Fix `is_local()` to reject `localhost.evil.com` as local

## Goal
The `is_local()` method must only match exact localhost hosts, not subdomains like `localhost.evil.com`.

## Approach
After stripping the scheme, check that the host portion is exactly "localhost", "127.0.0.1", or "[::1]" followed by ':', '/', or end-of-string. Replace `starts_with` with a helper that validates the boundary character.

Add 3 new tests: subdomain bypass (must be false), localhost with port, localhost with path.

## Files to Modify
- `packages/pipelin3r/src/executor/mod.rs` — fix `is_local()`
- `packages/pipelin3r/src/executor/tests.rs` — add new tests
