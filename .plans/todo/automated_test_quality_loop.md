# Automated Test Quality Loop: Fuzz → Mutate → Reduce → Repeat

## The Vision

A closed-loop pipeline step that takes a codebase with tests and automatically converges to a minimal, maximally-effective test suite. No human in the loop — runs until stable.

## The Loop

```
    ┌─────────────────────────────────────────────┐
    │                                             │
    ▼                                             │
1. FUZZ (cargo-fuzz)                              │
   Find crash inputs the tests miss               │
    │                                             │
    ▼                                             │
2. CASE REDUCE (creduce/preduce/delta-debug)      │
   Shrink each crash to minimal reproducer         │
    │                                             │
    ▼                                             │
3. ADD TO SUITE                                   │
   Each minimal crash becomes a new regression     │
   test: input → expected behavior (crash/error)   │
    │                                             │
    ▼                                             │
4. MUTATE (cargo-mutants)                         │
   Inject bugs into code, check if tests catch     │
   them. Surviving mutants = test gaps.            │
    │                                             │
    ▼                                             │
5. FILL GAPS                                      │
   For each surviving mutant, generate a test      │
   that kills it (LLM or targeted generation)      │
    │                                             │
    ▼                                             │
6. SUITE REDUCE                                   │
   Remove redundant tests. Per-test coverage →     │
   set cover → minimal set with same kill rate.    │
    │                                             │
    ▼                                             │
7. CHECK CONVERGENCE                              │
   - No new fuzzer crashes?                        │
   - No surviving mutants?                         │
   - No redundant tests?                           │
   If all three: DONE. Otherwise: loop back to 1. │
    │                                             │
    └──── not converged ──────────────────────────┘
```

## Convergence Guarantee

The loop converges because:
- Fuzzing finds a finite number of crash paths (code has finite branches)
- Mutation testing has a finite number of viable mutations
- Suite reduction only removes tests, never adds
- Each iteration either: adds a needed test, removes a redundant test, or finds no changes (stable)

In practice: 2-4 iterations to reach stability. First round finds most issues, subsequent rounds catch edge cases from the changes.

## What Exists Today (tools per step)

| Step | Tool | Language | Status |
|------|------|----------|--------|
| 1. Fuzz | cargo-fuzz / libFuzzer | Rust | Mature |
| 2. Case reduce | preduce (Rust), C-Reduce, delta-debug | Multi | Mature |
| 3. Add to suite | Custom (turn crash into #[test]) | - | Build |
| 4. Mutate | cargo-mutants | Rust | Mature |
| 5. Fill gaps | LLM agent (pipelin3r) | - | Build |
| 6. Suite reduce | **NOTHING FOR RUST** | - | **Build** |
| 7. Convergence | Custom (compare metrics) | - | Build |

## What We Need to Build

### A. `cargo-test-reduce` (Rust test suite reduction tool)

The missing piece. No Rust tool exists.

**Algorithm:**
1. Run each test individually with `cargo-llvm-cov` to get per-test coverage
2. Build a matrix: test → {lines covered}
3. Solve weighted set cover (greedy approximation):
   - Pick test that covers the most uncovered lines
   - Mark those lines as covered
   - Repeat until all lines covered
   - Remaining tests are redundant
4. Optionally: also consider mutation kill data (test → {mutants killed})

**Input:** A Rust crate with tests
**Output:** List of redundant tests with confidence scores, or the minimal set

**Approach options:**
- Standalone CLI tool (`cargo-test-reduce`)
- Or integrate into pipelin3r as a step

**Key dependency:** `cargo-llvm-cov` for per-test coverage. It supports `--tests` flag and JSON output. Need to parse the coverage per individual test function.

### B. Crash-to-test converter

Turns fuzzer crash inputs into `#[test]` functions.

**Input:** Crash input file + the fuzz target function signature
**Output:** A Rust test function that calls the parse function with the crash input and asserts it doesn't panic (or asserts specific error behavior)

This is mechanical — read the crash bytes, embed as a byte literal or include_bytes!, wrap in a test function. LLM not needed.

### C. Convergence checker

Compares metrics across iterations:
- Fuzzer: new unique crashes found (0 = stable)
- Mutants: surviving count (0 = stable)
- Suite: redundant test count (0 = stable)
- All three at 0 = converged

### D. Loop orchestrator

A pipelin3r pipeline that runs the loop. Each iteration is a sequence of steps:

```
fuzz_step → reduce_step → add_tests_step → mutate_step → fill_gaps_step → suite_reduce_step → check_convergence_step
```

If not converged, re-run. If converged, report final metrics.

## Pipeline Integration

New pipeline steps after implementation:

| # | Step | Type |
|---|------|------|
| 23 | fuzz | Programmatic (cargo-fuzz) |
| 24 | reduce_crashes | Programmatic (preduce/delta-debug) |
| 25 | crashes_to_tests | Programmatic (embed crash inputs as tests) |
| 26 | mutation_testing | Programmatic (cargo-mutants) |
| 27 | fill_mutation_gaps_llm | LLM (generate tests for surviving mutants) |
| 28 | suite_reduce | Programmatic (cargo-test-reduce, TO BE BUILT) |
| 29 | check_convergence | Programmatic (compare metrics) |
| 30 | loop_or_done | Programmatic (re-run 23-29 if not converged) |

## Expected Results

For a typical parser library (~5K lines, ~400 initial tests):
- Round 1: Fuzzer finds 10-20 crashes, mutants expose 30-50 test gaps, suite reduces by 15-25%
- Round 2: Fuzzer finds 2-5 more crashes (from new code paths revealed by better tests), mutants drop to <10, minor reduction
- Round 3: 0 new crashes, 0-2 surviving mutants, no redundancy. Converged.
- Final: ~350 tests (from initial 400), 95%+ mutation kill rate, zero known crash inputs, zero redundancy

## Research References

### Suite reduction tools (closest to what we need)
- TestIQ (Python) — https://github.com/pydevtools/TestIQ — redundancy detection via coverage
- FAST-R — https://github.com/ICSE19-FAST-R/FAST-R — scalable similarity-based reduction
- Nemo (Java) — https://github.com/jwlin/Nemo — multi-criteria minimization
- JSR (Java) — https://github.com/Lms24/JSR — greedy + genetic reduction
- GASSER — https://github.com/ccoviello/gasser — genetic algorithm reduction

### Set cover solvers (the core algorithm)
- SetCoverPy — https://github.com/guangtunbenzhu/SetCoverPy
- setcoveringsolver (C++) — https://github.com/fontanf/setcoveringsolver

### Case reduction
- preduce (Rust) — https://github.com/fitzgen/preduce
- C-Reduce — https://github.com/csmith-project/creduce
- Picire (parallel delta debugging) — https://github.com/renatahodovan/picire

### Mutation testing
- cargo-mutants (Rust) — https://github.com/sourcefrog/cargo-mutants
- PITest (Java gold standard) — https://github.com/hcoles/pitest
- Stryker (JS/TS/.NET) — https://github.com/stryker-mutator/stryker-js

## Priority

Build `cargo-test-reduce` first (the missing piece). Everything else exists. Once we have suite reduction, the loop can be assembled from existing tools + pipelin3r orchestration.
