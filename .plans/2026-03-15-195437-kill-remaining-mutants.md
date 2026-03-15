# Kill 19 surviving mutants in limit3r

**Date:** 2026-03-15 19:54
**Task:** Write targeted tests to kill 19 surviving mutants across 4 files

## Goal
All 19 mutants killed with new tests, `cargo test -p limit3r` passes.

## Approach

### duration_serde.rs (2 mutants)
Lines 28 and 298: `||` to `&&`. Existing tests already use `-1.0` which is negative-only. Check if they actually assert `is_err()` — they do. These should already be killed. But the mutant report says they survive, so need to verify the test structure. The existing `rejects_negative_duration` and `mutant_kill_option_rejects_negative` tests DO test `-1.0`. The issue may be that serde_json itself rejects the value before reaching our guard. Wait — no, `-1.0` is valid JSON f64. So the test should work. Let me re-read... The existing tests look correct. Perhaps the mutation tool is targeting specific lines and the existing tests don't cover them precisely. I'll add explicit tests that are clearly named for the `||` vs `&&` mutation to be safe.

Actually looking more carefully: the non-option tests in `non_option_guard_tests` already have `mutant_kill_non_option_or_vs_and` which tests `-1.0`. And `option_tests` has `mutant_kill_option_guard_all_conditions_independent` with `-0.5`. These should kill `||`->`&&`. If they survive, maybe we need to also test NaN-only and Infinity-only independently. Since serde_json can't represent NaN/Infinity, we need a custom deserializer or direct function call. Let me check if we can call `deserialize` directly with a mock.

Actually the problem is clear: serde_json CANNOT represent NaN or Infinity. So we can never test those conditions through serde_json. The `-1.0` test kills the `||`->`&&` mutation because with `&&`, `-1.0` (negative, not NaN, not infinite) passes all three conditions as false under `&&` (needs ALL true). Wait no: `secs < 0.0` is true, `secs.is_nan()` is false, `secs.is_infinite()` is false. With `||`: true || false || false = true (rejected). With `&&`: true && false && false = false (accepted). So `-1.0` SHOULD kill this mutant. The existing tests DO test this. So why do they survive?

Possibility: the mutation tool may be reporting false positives, OR the test names don't match what it looks for. Regardless, user says they survive. I'll add new, distinctly named tests.

### circuit_breaker.rs (5 mutants)
Lines 119, 124, 128. Need tests with 10,001+ entries triggering eviction with specific state assertions.

### rate_limiter.rs (9 mutants)
Lines 83, 85, 86, 87, 93, 135, 149. Similar eviction tests + deadline boundary + Debug fmt.

### retry.rs (2 mutants)
Line 50: `>` to `>=` — test delay exactly equal to max.
Line 52: `<` to `<=` — test delay exactly 0.0.

## Files to Modify
- `packages/limit3r/src/duration_serde.rs` — add 1 test
- `packages/limit3r/src/circuit_breaker.rs` — add 3 tests
- `packages/limit3r/src/rate_limiter.rs` — add 4 tests
- `packages/limit3r/src/retry.rs` — add 2 tests
