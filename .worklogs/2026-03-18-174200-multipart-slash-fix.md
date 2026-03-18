# Fix multipart upload: slashes in file paths

**Date:** 2026-03-18 17:42
**Scope:** shedul3r bundle handler, SDK upload

## Summary
actix multipart rejects part names containing `/` (e.g., `research/rust/overview.md`). Fixed: SDK uses numeric part names (`file0`, `file1`), server reads actual path from Content-Disposition filename.

## Root cause
reqwest's `form.part(name, part)` uses `name` as the multipart part name. actix's multipart parser can't handle slashes in part names — it fails with "Content-Disposition header was not found." This broke every upload with subdirectory paths.
