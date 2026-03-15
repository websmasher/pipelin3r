# Fix all code quality findings from review

**Date:** 2026-03-15 16:19
**Task:** Apply fixes for H1+H2, H3, M4, M5, M6, M7, M8, L12, L14, L15

## Goal
Fix all identified code quality issues across bundles.rs, bulkhead.rs, circuit_breaker.rs, rate_limiter.rs, client.rs, config.rs, agent.rs, ci.yml, and release-binary.yml.

## Approach

### H1+H2: Stream uploads in bundles.rs
- Add tokio `io-util` feature to workspace deps
- Replace `field.bytes().await` with `field.chunk()` loop
- Add `MAX_TOTAL_BUNDLE_SIZE` constant
- Track per-field and total bytes during streaming

### H3: TOCTOU in bulkhead.rs
- Remove read-lock-then-write-lock pattern, evict under write lock directly

### H3 also in circuit_breaker.rs and rate_limiter.rs
- circuit_breaker already uses write lock - eviction is fine
- rate_limiter uses Mutex - already atomic

### M4: Poll timeout in client.rs
- Cap initial delay to not exceed max_duration
- Check elapsed BEFORE sleeping in loop

### M5: Circuit breaker eviction - evict all Closed keys
### M6: Rate limiter eviction - evict oldest when expired eviction insufficient
### M7: Version sync CI job
### M8: Add x86_64-apple-darwin and aarch64-unknown-linux-gnu targets
### L12: backoff_multiplier infinity check
### L14: Blocking fs in async (agent.rs)
### L15: Extract remote bundle helper (agent.rs)

## Files to Modify
- `apps/shedul3r/Cargo.toml` - add io-util to tokio features
- `apps/shedul3r/crates/adapters/inbound/api/src/handlers/bundles.rs`
- `packages/limit3r/src/bulkhead.rs`
- `packages/limit3r/src/circuit_breaker.rs`
- `packages/limit3r/src/rate_limiter.rs`
- `packages/shedul3r-rs-sdk/src/client.rs`
- `packages/limit3r/src/config.rs`
- `packages/pipelin3r/src/agent.rs`
- `.github/workflows/ci.yml`
- `.github/workflows/release-binary.yml`
- `packages/shedul3r-bin/Cargo.toml`
