# Rewrite s04, s05, s07 to run all commands remotely via shedul3r

**Date:** 2026-03-19 12:42
**Task:** Rewrite step files s04, s05, s07 to use remote execution via shedul3r instead of local `std::process::Command`.

## Goal
All three step files should execute their work on the remote shedul3r server using `config.executor.run_remote_command()` and `config.executor.run_agent()` instead of spawning local processes.

## Approach

### s04_clone_and_extract.rs
- Phase 1: Clone repos remotely via `run_remote_command` with `git clone` commands
- Phase 2: Install dependencies remotely via `run_remote_command` with language-specific install commands
- Phase 3: Generate wrappers via `run_agent` — one agent call per library that generates, writes, and tests the wrapper on the remote machine

### s05_extract_source.rs
- Simple remote health check: `ls -la {remote_clones_dir}/` via `run_remote_command`
- Change signature to async

### s07_run_parsers.rs
- Discover fixtures locally from -tests.json files
- Create temp dir with fixture .txt files
- Submit ONE `run_remote_command` with work_dir containing fixtures
- Remote bash script runs all wrappers on all fixtures, outputs JSON summary to stdout
- Parse stdout JSON and write results locally to ground-truth/

## Files to Modify
- `websmasher/tools/dev-process-v3/src/steps/s04_clone_and_extract.rs`
- `websmasher/tools/dev-process-v3/src/steps/s05_extract_source.rs`
- `websmasher/tools/dev-process-v3/src/steps/s07_run_parsers.rs`
