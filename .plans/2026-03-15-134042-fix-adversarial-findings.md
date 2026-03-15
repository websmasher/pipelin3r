# Fix all adversarial review findings

**Date:** 2026-03-15 13:40
**Task:** Fix 5 critical, 9 high, key medium findings across all packages

## Fixes by package

### shedul3r (CRITICAL)
- S1-S2: Path traversal — canonicalize paths, reject absolute paths, verify result is inside bundle dir
- S3: Add body size limit (DefaultBodyLimit layer), stream to disk, max bundle count
- S4: Remove server path from upload response

### limit3r (CRITICAL + HIGH)
- L1: Document bulkhead acquire/release contract, add validation
- L4: Add key eviction (max keys or TTL-based cleanup)
- L9: Preserve last error in RetryExhausted variant
- L3/L8: Validate zero config values (limit_for_period=0, max_attempts=0, sliding_window_size=0)
- L13: Validate duration_serde against negative/NaN values

### pipelin3r (CRITICAL + HIGH)
- P-C1: Auth::FromEnv returns error (not empty map) when no env vars found
- P-C3: Template — apply content replacements in single pass (not sequentially) to prevent cross-injection
- P-H1: Remote bundle cleanup in finally/drop, not after ? propagation
- P-H2: Replace tokio full with explicit features
- P-H3: Replace anyhow with thiserror in public APIs
- P-H4: Replace HashMap with BTreeMap
- P-H5: Use tempfile::tempdir() instead of PID-based names

### SDK (CRITICAL)
- P-C2: Add max poll duration to poll_for_file
