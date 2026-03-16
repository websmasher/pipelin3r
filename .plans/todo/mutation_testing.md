# Mutation Testing: Validate Test Quality with cargo-mutants

## Problem

After implementing a library through the pipeline (research → steal tests → rewrite in Rust → steal code → implement in Rust → tests pass), we don't actually know if the tests are GOOD. Tests passing means the code matches the tests — but the tests could be:
- Asserting nothing meaningful (weak assertions)
- Not catching regressions if the code changes (brittle)
- Missing important behavioral checks
- Passing by coincidence (correct for wrong reasons)

## Solution: Mutation Testing

**Tool:** [cargo-mutants](https://mutants.rs/) (`cargo install cargo-mutants`)

**What it does:** Systematically injects bugs into the source code (mutations) and checks if the test suite catches them. If a mutation survives (tests still pass with the bug injected), the test suite has a gap.

**Example mutations:**
- Replace `if x > 0` with `if x >= 0` — does a test catch this?
- Replace `return Ok(value)` with `return Ok(Default::default())` — does a test notice?
- Delete a function body, return default — does anything break?

A "surviving mutant" = a bug that tests don't catch = a test quality gap.

## When to Run

After Phase 5 (implement parser) when all tests pass. NOT before — mutation testing requires a green test suite as baseline.

Pipeline order:
```
Phase 1-3: Research → Tests → Implement tests (439 tests, all compile)
Phase 4-5: Plan → Implement parser (tests start passing)
Phase 6: Integration testing, fuzzing

>>> MUTATION TESTING HERE <<<

- All tests green
- Run cargo-mutants on the entire package
- Review surviving mutants
- Add tests to kill surviving mutants
- Repeat until mutation score is acceptable (>80%)
```

## How to Run

```bash
# Install
cargo install cargo-mutants

# Run on a specific package (e.g., security-txt-parser)
cargo mutants -p websmasher-security-txt-parser

# Run on the whole workspace
cargo mutants --workspace

# Parallel (faster)
cargo mutants -p websmasher-security-txt-parser -j 4

# Output: list of surviving mutants with file + line + mutation description
```

## What to Do with Results

**For each surviving mutant:**
1. Is the mutation semantically equivalent to the original? (e.g., `x + 0` → `x - 0` when x is always 0). If yes → skip with `#[mutants::skip]` and document why.
2. Is the mutation a real behavior change that tests should catch? If yes → write a test that kills it.
3. Is the mutation in test-only or config code? If yes → lower priority.

**Target:** >80% mutation kill rate for parser/library code. 100% is impractical (some mutations are semantically equivalent).

## Integration into Pipeline

This becomes a new pipeline step after implementation:

| # | Step | Type |
|---|------|------|
| 19 | verify_implementation | Programmatic (cargo test) |
| **19b** | **mutation_testing** | **Programmatic (cargo-mutants)** |
| 20 | golden_files_llm | LLM |

The step:
1. Runs `cargo mutants -p {package} --output {research}/mutation-report.json`
2. Parses the report
3. For each surviving mutant in library code (not tests): flags as a gap
4. Reports mutation score
5. If score < threshold: generates targeted tests for the gaps (LLM agent per gap group)

## Why This Matters for the Pipeline

The entire pipeline extracts tests from 30+ source libraries and translates them to Rust. But:
- Translation can lose assertion precision
- LLM-generated tests can assert wrong things
- Deduplication can remove tests that covered unique edge cases
- Adversarial expansion can add tests that look good but assert nothing

Mutation testing is the ONLY way to verify that the test suite actually validates behavior, not just exercises code paths. Coverage tells you "this line was reached." Mutation testing tells you "this line's behavior is verified."

## Performance Expectations

For a ~5K line parser with ~400 tests:
- ~200-500 viable mutations (depends on code structure)
- Each mutation: ~2-5 seconds (compile + test)
- Total: ~10-30 minutes with parallelism
- Survivals: expect 20-50% initially (LLM-generated tests are notoriously weak on assertions)

After adding tests for survivors, target <20% survival rate.

## Config

No special config needed. Optional: add to `Cargo.toml`:
```toml
# Speed up mutation testing builds
[profile.mutants]
inherits = "dev"
debug = false        # skip debug symbols
incremental = true   # reuse across mutations
```

## Also Run on These Libraries

Not just the parser packages — run on pipelin3r infrastructure too:
- `cargo mutants -p limit3r` — verify resilience pattern tests
- `cargo mutants -p shedul3r-rs-sdk` — verify SDK client tests
- `cargo mutants -p pipelin3r` — verify orchestration tests

The 217 tests we have should kill most mutations, but the adversarial reviews found multiple bugs the tests didn't catch — mutation testing would have flagged those.
