# Fix all adversarial review v2 findings

**Date:** 2026-03-15 17:12

## Batch 1: limit3r + shedul3r dedup (Agent 1)

### CRITICAL: Circuit breaker premature trip (L-BUG1)
- Add minimum call threshold before evaluating failure rate
- If results.len() < minimum_calls (e.g., sliding_window_size / 2, or a min of 5), don't evaluate rate — return Ok
- Fix in BOTH packages/limit3r/src/circuit_breaker.rs AND apps/shedul3r adapter copy

### HIGH: shedul3r duplicates limit3r (L-DUP1)
- The ENTIRE POINT of limit3r was to eliminate this duplication
- apps/shedul3r/crates/adapters/outbound/db/ should import and re-export limit3r implementations, NOT reimplement them
- Change db/src/lib.rs to: `pub use limit3r::{InMemoryRateLimiter, InMemoryCircuitBreaker, InMemoryBulkhead, TokioRetryExecutor};`
- Delete the duplicate implementations from db/src/{rate_limiter,circuit_breaker,bulkhead,retry}.rs
- Keep db/Cargo.toml depending on limit3r
- Verify all shedul3r tests still pass

### HIGH: Bulkhead permit leak on evicted key (L-BUG4)
- When a key is evicted between acquire and release, the permit is lost
- Fix: don't evict keys that have outstanding permits (available < max)
- This is already the eviction criterion... but the issue is: between acquire() calling get_or_create_semaphore (which evicts) and the actual semaphore.acquire(), another call could evict the key
- Better fix: eviction should NEVER remove keys where available_permits < max_concurrent (there are outstanding borrows)
- The current retain already does this — `entry.semaphore.available_permits() < max` means "has outstanding permits, keep it"
- The real issue: after forget() the permit count drops, but between get_or_create returning the Arc and acquire() getting the permit, the entry could be evicted by another thread's get_or_create call
- Fix: the Arc<Semaphore> survives eviction because the caller holds an Arc. So release() must NOT look up the map — it should use the Arc directly
- This requires changing the Bulkhead trait to return a guard or the Arc. Since we can't change the trait without breaking API, document this as a known limitation for now and add a warning log in release() when the key is missing

### MEDIUM: Circuit breaker > vs >= (L-BUG2)
- Change `rate > config.failure_rate_threshold` to `rate >= config.failure_rate_threshold`

### MEDIUM: Missing tests for eviction, config validation, duration_serde, concurrent access
- Add tests for eviction behavior
- Add tests for config validate() methods
- Add tests for duration_serde edge cases (negative, NaN, infinity)

## Batch 2: shedul3r server fixes (Agent 2)

### CRITICAL: No auth (S-S1, S-S2)
- Add optional API key authentication via middleware
- New env var: SHEDUL3R_API_KEY — if set, all requests must include `Authorization: Bearer {key}` header
- If not set, no auth (backward compatible for local dev)
- Axum middleware layer that checks the header

### HIGH: 50MB global vs 100MB bundle (S-L1)
- Either raise global to 100MB, or add per-route body limit override for bundle endpoint
- Best: use Axum's per-route DefaultBodyLimit: `.route("/api/bundles", post(...).layer(DefaultBodyLimit::max(100_000_000)))`

### HIGH: No validation on working_directory (S-S3)
- Validate working_directory exists and is a directory
- Reject absolute paths outside a configurable allowed prefix (env var SHEDUL3R_ALLOWED_WORKDIR_PREFIX, default: no restriction)

### MEDIUM: Exit code always 1 (S-L4)
- Fix build_task_response to preserve actual exit code from SubprocessResult

### MEDIUM: Bundle download not streamed (S-L5)
- Use tokio::fs::File + axum::body::Body::from_stream for streaming response

### MEDIUM: Circuit breaker config hardcoded (S-L3)
- Add circuit-breaker section to YAML parser, same pattern as rate-limit and retry

### MEDIUM: limiter_status() stubbed (S-L2)
- Wire to actual adapter state (read from InMemoryRateLimiter/CircuitBreaker/Bulkhead)

### LOW: Inconsistent error format, sqlx dead dep, module visibility

## Batch 3: SDK fixes (Agent 3)

### HIGH: http_call never returns Err (SDK-12)
- Redesign: network errors should be Err, task failures should be Ok(TaskResult{success:false})
- reqwest send/parse errors → Err(SdkError::Http)
- Task reported failure → Ok(TaskResult{success:false})

### HIGH: Bundle path not URL-encoded (SDK-8)
- Use reqwest URL builder or percent-encode the path segments

### HIGH: File-poll recovery untested (SDK-14)
- Add integration test with a temp file that appears after a delay

### MEDIUM: SDK missing limiter_key, timeout_ms fields
- Add to TaskPayload

### MEDIUM: Response metadata dropped
- Add metadata fields to TaskResult (exit_code, elapsed, started_at)

### MEDIUM: Error responses not correctly parsed
- Check HTTP status code, parse error body format

### LOW: Dead error variants (TaskFailed, Json, Io)
- Wire them up or remove them

## Batch 4: pipelin3r fixes (Agent 4)

### HIGH: Bundle path traversal (P-9)
- Add validate_bundle_path (same as shedul3r) to write_to_temp_dir

### HIGH: Phase-1 template cross-injection (P-3)
- Either: make phase-1 also single-pass, or reject values containing {{ in set()

### MEDIUM: Tool enum not implemented
- Create Tool enum, update AgentBuilder::tools() signature

### MEDIUM: Dry-run missing auth + bundle capture
- Write env vars (redacted values) and bundle file list to meta.json

### MEDIUM: PipelineError::Other overused
- Add specific variants: AgentFailed, BatchPartialFailure, Timeout

### MEDIUM: TemplateFiller uses &mut self not self (ergonomics)
- Change to consume self for chaining: fn set(self, ...) -> Self

### MEDIUM: No Template::from_file
- Add convenience method

### LOW: executor.command() and executor.transform() don't exist on Executor
- Add forwarding methods for consistency with plan
