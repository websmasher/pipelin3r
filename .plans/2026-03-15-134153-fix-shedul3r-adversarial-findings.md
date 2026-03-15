# Fix shedul3r adversarial security review findings

**Date:** 2026-03-15 13:41
**Task:** Fix all adversarial review findings in shedul3r bundles handler

## Goal
Harden the bundle upload/download handlers against path traversal, add request body size limits, add orphan/TTL comments, and add tests for all security validations.

## Approach

### Step-by-step plan
1. **bundles.rs** — Add `validate_bundle_path()` using `Path::components()` that only allows `Component::Normal`. Apply to both upload field names and download paths.
2. **bundles.rs upload** — Add file count limit (max 100 files). Add per-field size check.
3. **bundles.rs upload** — Add comment about path being intentionally included for SDK `working_directory` flow.
4. **main.rs** — Add `DefaultBodyLimit::max(50_000_000)` layer to the router.
5. **bundles.rs** — Add TODO comments about orphaned TempDirs and TTL reaper.
6. **bundles.rs tests** — Add path traversal tests: absolute path, parent traversal, normal nested, mixed traversal.

### Key decisions
- Use `Component::Normal`-only validation — cleanest approach, rejects all dangerous path components
- Keep `path` in upload response — needed by SDK for `working_directory`
- 50MB body limit, 100 file limit, 10MB per-field limit

## Files to Modify
- `apps/shedul3r/crates/adapters/inbound/api/src/handlers/bundles.rs` — path validation, file limits, tests
- `apps/shedul3r/crates/adapters/inbound/api/src/main.rs` — body size limit layer
