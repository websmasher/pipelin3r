# Parallel pipeline rewrite: steps 4 + 6

**Date:** 2026-03-19 21:50
**Scope:** s04_clone_and_install.rs, s06_run_parsers.rs, config.rs, Dockerfile

## Summary
Rewrote steps 4 and 6 per the handoff plan: full parallelization via run_pool_map, isolated per-library environments, structured test output parsing, strict error handling with retries.

## Context & Problem
Steps 4 and 6 were sequential (for loops), used shared system packages, and parsed test output with regex on stdout. The pipeline needed to work unattended for any parser across ~70 future libraries.

## Decisions Made

### Parallelization
- **Chose:** run_pool_map with concurrency = total (all libraries at once)
- **Why:** Speed comes from parallelism. Each library is independent.

### Isolated environments
- **Chose:** .venv, .gopath, .cargo, .bundle inside each clone dir
- **Why:** No shared system packages, no version conflicts between libraries
- **Removed:** global GOPATH env var from Dockerfile, remote_env_preamble() from config

### Structured test output
- **Chose:** Native formats (JUnit XML, go test -json, Jest JSON, RSpec JSON, TRX XML, cargo test text)
- **Why:** Regex on stdout breaks on format variations. Structured output is guaranteed parseable.
- **Fallback:** Regex on stdout only when structured file is missing

### Error handling
- **Chose:** Fail the step if ANY library fails, retry 2x with 3s delay
- **Why:** Empty results hide failures. Every library must succeed or the pipeline stops and tells you what broke.
- **Added:** Disk space check before clone/install (fails early if <1GB free)

## Key Files for Context
- `.plans/2026-03-19-202656-pipeline-robustness-handoff.md` — the design plan
- `websmasher/tools/dev-process-v3/src/steps/s04_clone_and_install.rs` — parallel clone/install/extract
- `websmasher/tools/dev-process-v3/src/steps/s06_run_parsers.rs` — parallel test suites with structured output
- `websmasher/tools/dev-process-v3/src/config.rs` — build_install_script(), lang support
- `claude-worker/Dockerfile` — .NET 8 SDK, phpunit, mocha, no global GOPATH
