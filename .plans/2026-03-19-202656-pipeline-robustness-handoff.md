# Pipeline Robustness: Handoff Document

**Date:** 2026-03-19 20:26
**Status:** Design agreed, ready for implementation

## Problem Statement

The pipeline must work for ANY parser library (not just security.txt) across ~70 future parsers. Every run must complete end-to-end without human intervention. The current implementation has:
- Sequential steps that should be parallel
- Shared environments that conflict between libraries
- Missing dependencies that crash instead of auto-installing
- Regex-based test output parsing that breaks on format variations
- No idempotency — re-runs repeat completed work

## Core Principles

1. **Every library gets its own isolated environment.** No shared site-packages, no shared GOPATH, no shared cargo registry. Each library's install + test runs in its own sandbox.
2. **Maximum concurrency everywhere.** If things don't depend on each other, they run at the same time. The only limit is what the worker machine can physically handle.
3. **Auto-healing.** If a dependency is missing, install it. If a test runner isn't found, try alternatives. If a version is wrong, install the right one. Never crash on a fixable problem.
4. **Fast through parallelism, not skipping.** Never skip a step because output exists — it might be broken. Instead, make every step fast by running all libraries in parallel. Individual operations are naturally idempotent (git clone checks if dir exists, pip is a no-op on installed packages), but the step itself always runs.
5. **One-shot.** Run `--step all` and walk away. Come back to golden files.

---

## Step-by-Step Redesign

### Step 1: Scaffold (VerifiedStep) — no change needed
- Single agent call, already works
- Concurrency: N/A (single step)

### Step 2: Find Libraries (VerifiedStep batch) — no change needed
- 9 languages in parallel, already uses `run_verified_step_batch`
- Concurrency: 9 (one per language)

### Step 3: Extract Libraries JSON (VerifiedStep batch) — no change needed
- 9 languages in parallel, already works
- Concurrency: 9

### Step 4: Clone + Install — MAJOR REWRITE

**Current:** Sequential for loop, 63 remote commands one at a time, shared system packages.

**New design:**

#### Phase 4a: Clone all repos (parallel)
- `run_pool_map` with concurrency = total (all 21 at once)
- Each clone is idempotent: `if [ -d ... ]; then echo skip; else git clone; fi`
- One `RemoteCommandConfig` per library, all fire simultaneously
- Takes ~10s total instead of ~3.5 minutes

#### Phase 4b: Install all libraries (parallel, isolated environments)
- `run_pool_map` with concurrency = total
- Each library gets a SINGLE remote command that does everything:

**Python:**
```sh
cd $CLONE_DIR
python3 -m venv .venv
.venv/bin/pip install -e ".[dev,test]" 2>/dev/null
.venv/bin/pip install -e ".[test]" 2>/dev/null
.venv/bin/pip install -e . 2>/dev/null
.venv/bin/pip install pytest 2>/dev/null
echo "INSTALLED"
```

**Go:**
```sh
cd $CLONE_DIR
export GOPATH=$CLONE_DIR/.gopath
go mod download 2>/dev/null || true
echo "INSTALLED"
```

**Rust:**
```sh
cd $CLONE_DIR
export CARGO_HOME=$CLONE_DIR/.cargo
cargo fetch 2>/dev/null || true
echo "INSTALLED"
```

**PHP:**
```sh
cd $CLONE_DIR
composer install --no-interaction 2>/dev/null || true
echo "INSTALLED"
```

**Ruby:**
```sh
cd $CLONE_DIR
bundle install --path .bundle 2>/dev/null || true
echo "INSTALLED"
```

**JavaScript:**
```sh
cd $CLONE_DIR
npm install 2>/dev/null || true
echo "INSTALLED"
```

**C#:**
```sh
cd $CLONE_DIR
# Detect target framework from .csproj files
TFM=$(grep -r '<TargetFramework>' --include='*.csproj' -h | head -1 | sed 's/.*<TargetFramework>\(.*\)<.*/\1/')
CHANNEL=$(echo $TFM | sed 's/net//' | cut -d. -f1-2)
# Install required SDK if not present
if ! dotnet --list-sdks | grep -q "^${CHANNEL}"; then
    curl -fsSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel $CHANNEL --install-dir /data/pipelin3r/tools/dotnet
fi
dotnet restore 2>/dev/null || true
echo "INSTALLED"
```

Key differences:
- `.venv`, `.gopath`, `.cargo`, `.bundle` all inside the clone dir — fully isolated
- Each command is self-contained — installs everything it needs, never depends on system state
- All 21 run at the same time
- Idempotent — `.venv/bin/python3` already exists? Skip.

#### Phase 4c: Extract test data (parallel)
- `run_pool_map` with concurrency = total
- Run `extract-tools tests $CLONE_DIR $LANG` for each library
- Save output locally to `research/{slug}/{lib}-tests.json`
- All 21 at once

### Step 5: Generate Wrappers (LLM agents) — minor change
- Already parallel via `run_pool_map`
- Change: each wrapper prompt should reference the isolated environment:
  - Python wrappers use `.venv/bin/python3` not system python
  - Go wrappers use `GOPATH=$CLONE_DIR/.gopath`
  - etc.
- Concurrency: limited by Claude API rate limit (shedul3r handles this via provider key `claude`)

### Step 6: Run Test Suites — MAJOR REWRITE

**Current:** Sequential remote commands, regex parsing of stdout, shared system packages.

**New design:**

One remote command per library via `run_pool_map`. Each command is a self-contained script that:
1. Activates the library's isolated environment
2. Runs the test suite
3. Writes structured results to a file in the work_dir
4. Downloads via bundle

**Per-language test scripts:**

**Python:**
```sh
cd $CLONE_DIR
.venv/bin/python3 -m pytest --junitxml=/work/test-output.xml -v 2>&1 || true
```
Parse JUnit XML — guaranteed structured format, no regex.

**Go:**
```sh
cd $CLONE_DIR
export GOPATH=$CLONE_DIR/.gopath
go test -json ./... > /work/test-output.json 2>/dev/null || true
```
Parse JSON lines — guaranteed structured format.

**Rust:**
```sh
cd $CLONE_DIR
export CARGO_HOME=$CLONE_DIR/.cargo
cargo test 2>&1 | tee /work/test-output.txt || true
```
Parse `test X ... ok/FAILED/ignored` — simple, reliable regex on cargo's stable output format.

**PHP:**
```sh
cd $CLONE_DIR
vendor/bin/phpunit --log-junit /work/test-output.xml 2>&1 || true
```
Parse JUnit XML.

**Ruby:**
```sh
cd $CLONE_DIR
bundle exec rspec --format json --out /work/test-output.json 2>&1 || true
```
Parse RSpec JSON.

**JavaScript:**
```sh
cd $CLONE_DIR
npx jest --json --outputFile=/work/test-output.json 2>&1 || true
```
Parse Jest JSON.

**C#:**
```sh
cd $CLONE_DIR
export DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1
dotnet test --logger "trx;LogFileName=/work/test-output.trx" 2>&1 || true
```
Parse TRX XML.

**Key differences from current:**
- Output files go to `/work/` (the uploaded work_dir), not to the clone dir. This guarantees they're downloadable via the bundle mechanism.
- Each language uses its **native structured output format**. No regex on stdout.
- The Python parsing script reads from the downloaded file, not from shedul3r stdout (no 32KB truncation).
- Isolated environments — `.venv/bin/pytest` not system `pytest`. No version conflicts.

**Concurrency:** All 21 at once. Test suites are CPU-bound on the worker but they're each in their own directory, no conflicts.

**Idempotency:** Check if `ground-truth/{lib}/test-results.json` already exists locally. If yes, skip.

### Step 7: Classify Results — no change needed
- Local step, reads JSON files, pure computation
- Already works correctly

### Step 8: Research Spec — no change needed
- Single VerifiedStep with adversarial breaker
- Already designed correctly

---

## Implementation Changes

### 1. Parallelize step 4 (clone, install, extract)

Replace the sequential `for entry in &libraries` loops with `run_pool_map`:

```rust
// Clone all at once
let clone_results = run_pool_map(
    libraries.clone(),
    libraries.len(), // full concurrency
    libraries.len(),
    |entry, idx, total| async move {
        // git clone --depth 1 ...
    }
).await;

// Install all at once
let install_results = run_pool_map(
    libraries.clone(),
    libraries.len(),
    libraries.len(),
    |entry, idx, total| async move {
        // isolated install script
    }
).await;

// Extract all at once
let extract_results = run_pool_map(
    libraries.clone(),
    libraries.len(),
    libraries.len(),
    |entry, idx, total| async move {
        // extract-tools tests ...
    }
).await;
```

### 2. Isolated environments in step 4 install

Add `build_install_script(lang, clone_dir)` function that returns a self-contained shell script per language. The script:
- Creates an isolated environment inside the clone dir
- Installs the library and all its dependencies (including dev/test)
- Installs the test runner if not already available
- Auto-detects and installs the right SDK version (for .NET)
- Is idempotent — checks if already done before doing work
- Never uses system-level pip/gem/npm — always per-project

### 3. Structured output in step 6

Replace the generated Python script with per-language test runner scripts. Each script:
- Activates the isolated environment
- Runs the test suite with structured output to a known path in the work_dir
- The results file is declared in `expect_outputs`
- A single Python parsing script (also in the work_dir) reads the structured output and writes `test-results.json`
- The parsing handles each format (JUnit XML, JSON lines, Jest JSON, TRX XML, RSpec JSON, cargo verbose)

### 4. No skip logic — make it fast instead

Do NOT skip steps based on existing output files. If something was broken on a previous run, skipping means it stays broken forever. Instead, always re-run everything but make it fast enough that it doesn't matter:

- `git clone --depth 1` is a no-op if the directory exists (the `if [ -d ... ]` check)
- `pip install` is a no-op if packages are already installed
- `cargo fetch` uses the local cache
- `bundle install` uses the local cache
- `npm install` uses the local cache

Speed comes from parallelization, not from skipping. Every run produces fresh golden files.

### 5. Auto-healing in the Dockerfile

The Dockerfile should include every tool that any library might need:
- Multiple .NET SDK versions (8.0, 9.0, 10.0) — some projects target older frameworks
- phpunit globally so it's available even if not in composer.json
- pytest, jest, rspec, mocha all globally available as fallbacks
- cmake, libssl-dev, libxml2-dev, etc. for native extensions
- The dotnet-install.sh script available on the volume for on-demand SDK installation

### 6. Per-library error tolerance

Step 4 and step 6 should never fail the pipeline because one library fails. Libraries that can't install or can't run tests get logged as warnings and produce empty golden files. The pipeline continues.

Current behavior already does this (`if ok == 0 && total > 0 { Err }` only fails if ALL libraries fail). Keep this.

---

## Expected performance after changes

| Step | Current | After |
|------|---------|-------|
| Step 4 clone | 3.5 min (21 × 10s sequential) | ~15s (all parallel, most skip) |
| Step 4 install | 6 min (21 × ~17s sequential) | ~2 min (all parallel, slowest is Rust compile) |
| Step 4 extract | 3.5 min (21 × 10s sequential) | ~15s (all parallel) |
| Step 5 wrappers | ~15 min (21 agents, concurrency 7) | Same — limited by Claude API |
| Step 6 test suites | ~3 min (21 × ~8s sequential) | ~30s (all parallel) |
| **Total steps 4-6** | **~31 min** | **~18 min** (dominated by step 5 LLM calls) |

---

## Files to modify

| File | Change |
|------|--------|
| `s04_clone_and_install.rs` | Full rewrite: parallel `run_pool_map`, isolated environments, idempotent |
| `s06_run_parsers.rs` | Full rewrite: parallel, structured output, isolated envs, idempotent |
| `config.rs` | Add `build_install_script(lang, clone_dir)` and `build_test_script(lang, clone_dir)` helpers |
| `Dockerfile` | Add .NET 8.0 SDK, ensure all global test tools present |
| `main.rs` | No change |

---

## Test strategy for the rewrite

Before sending ANY script to the remote worker:
1. Generate the script locally
2. `python3 -c "compile(open('script.py').read(), 'script.py', 'exec')"` — syntax check
3. If the script is a shell script, `bash -n script.sh` — syntax check
4. Only then upload and execute

This prevents the "run, wait 5 minutes, find a missing import, fix, re-run" cycle.
