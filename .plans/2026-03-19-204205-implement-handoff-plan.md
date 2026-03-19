# Implement handoff plan: parallel steps, isolated envs

**Date:** 2026-03-19 20:42
**Task:** Implement the pipeline robustness handoff plan

## Goal
Rewrite s04 and s06 so all operations are parallel with isolated per-library environments. Add .NET 8 to Dockerfile. Pipeline should work unattended for any parser.

## Approach

### Step 1: Add `build_install_script(lang, clone_dir)` to config.rs
Returns a self-contained shell script per language that creates an isolated environment inside the clone dir (.venv, .gopath, .cargo, .bundle, node_modules) and installs all dependencies including test deps.

### Step 2: Rewrite s04_clone_and_install.rs
- `verify_runtimes()` — no change
- `clone_repos()` — replace `for` loop with `run_pool_map`, concurrency = total (all at once)
- `install_dependencies()` — replace `for` loop with `run_pool_map`, use `build_install_script()`
- `extract_test_data()` — replace `for` loop with `run_pool_map`

### Step 3: Update s06 test commands to use isolated envs
- Python: `.venv/bin/python3 -m pytest` instead of system `python3 -m pytest`
- Go: `GOPATH=$CLONE_DIR/.gopath`
- Rust: `CARGO_HOME=$CLONE_DIR/.cargo`
- Ruby: `bundle exec` with `.bundle` path
- PHP: `vendor/bin/phpunit` (already correct)
- JS: `npx` (already correct, uses local node_modules)
- C#: auto-detect TFM, install SDK if needed

### Step 4: Dockerfile — add .NET 8 SDK
Add `--channel 8.0` install alongside existing 9.0 and 10.0.

## Files to Modify
- `websmasher/tools/dev-process-v3/src/config.rs` — add `build_install_script()`
- `websmasher/tools/dev-process-v3/src/steps/s04_clone_and_install.rs` — parallel rewrite
- `websmasher/tools/dev-process-v3/src/steps/s06_run_parsers.rs` — isolated env paths
- `claude-worker/Dockerfile` — .NET 8 SDK
