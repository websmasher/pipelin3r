# Kill 28 surviving mutants with targeted tests

**Date:** 2026-03-15 19:37
**Task:** Write tests to kill all 28 surviving mutants in shedul3r-rs-sdk (7) and pipelin3r (21)

## Goal
Every surviving mutant should be killed by a new test that directly exercises the mutated expression.

## Approach

### shedul3r-rs-sdk (7 mutants)

**client.rs:**
1. `base_url()` replaced with "" or "xyzzy" — test that Client::base_url() returns the configured URL
2. `== with !=` on line 304 — test that success=true in API response produces TaskResult.success=true (use deserialized ApiResponse directly)
3. `> with >=` on line 365 — already covered by `truncate_str_exact_boundary` test but mutant uses `>=` which would still pass for `<=`. Need a string of length max_len+1 to distinguish `>` from `>=`.

Wait, re-reading: line 365 is `while end > 0`, the loop condition. If mutated to `>=`, the loop would exit one iteration early (at end=0 it wouldn't execute). But end=0 only matters for non-char-boundary cases. The existing `truncate_str_multibyte_boundary` test with max_len=4 should catch this... unless the mutant is specifically `s.len() <= max_len` changed to `s.len() < max_len` on line 361. Let me re-check: line 361 is `if s.len() <= max_len` — if mutated to `<`, a string of exactly max_len would go to the else branch and get truncated. The existing test `truncate_str_exact_boundary` checks this but the assertion may not distinguish. Actually the existing test asserts `truncate_str("abcde", 5) == "abcde"` which WOULD fail if `<=` became `<`. So the surviving mutant must be on line 365: `while end > 0` changed to `while end >= 0`. But end is usize, so `>= 0` is always true — infinite loop. That can't survive. Let me just trust the user's description: `> with >=` on line 365. I'll add a test that exercises the boundary where end reaches 0.

**bundle.rs (lines 68, 105, 133):**
Need a mock HTTP server that returns specific status codes. Use `wiremock` or a simple TCP server that returns a raw HTTP response. Simplest: bind a TCP listener, accept, write a crafted HTTP response.

### pipelin3r (21 mutants)

**agent.rs:**
- AgentTask::bundle() returns Default::default() — verify bundle_data is Some after setting
- tools empty check `> with <` etc — verify tools presence/absence in task YAML via dry-run
- resolve_model_string — verify model string appears in dry-run task YAML
- batch partial failure — create a batch with mixed results, verify counts
- format_duration — already tested for 0 and nonzero, but need to verify `> 0` vs `< 0` etc. Add test for exactly 1h0m0s and 0h1m0s boundaries.

**executor.rs:**
- default_provider() returns None — verify after with_default_provider
- is_remote() — verify false by default, true after with_remote()

**pool.rs:**
- `== with !=` on line 31 — `if concurrency == 0 { 1 }` — test with concurrency=0

**template.rs:**
- overlapping keys — test with keys where one is prefix of another

## Files to Modify
- `packages/shedul3r-rs-sdk/src/client.rs` — add 3 tests
- `packages/shedul3r-rs-sdk/src/bundle.rs` — add 3 tests
- `packages/pipelin3r/src/agent.rs` — add 5 tests
- `packages/pipelin3r/src/executor.rs` — add 3 tests
- `packages/pipelin3r/src/pool.rs` — add 1 test
- `packages/pipelin3r/src/template.rs` — add 1 test

May need to add `wiremock` or use raw TCP for HTTP mocking in shedul3r-rs-sdk.
