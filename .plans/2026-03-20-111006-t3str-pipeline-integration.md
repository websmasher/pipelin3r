# Phase 4: Replace Python Scripts with t3str in dev-process-v3

## Goal

Replace the generated Python test scripts and `extract-tools` calls in the websmasher `dev-process-v3` pipeline with `t3str extract` and `t3str run`. This eliminates ~400 lines of generated Python, removes regex-based parsing, and uses compiled Rust with proper AST/structured parsers.

## Changes

### 1. s06_run_parsers.rs — Replace Python script with t3str run

**Current**: Generates a 500-line Python script per library that:
- Runs language-specific test commands
- Parses JUnit XML, Go JSON, cargo text, RSpec JSON, Jest JSON, TRX XML, mocha text
- Falls back to regex on stdout
- Writes test-results.json

**New**: Single shell command per library:
```
t3str run --language {lang} --repo-dir '{clone_dir}' --format json
```

**Changes**:
- Delete `build_test_script()` entirely
- Remove temp dir creation + Python syntax validation
- Run t3str via RemoteCommandConfig (no work_dir, no expect_outputs)
- Capture `result.output` (stdout JSON) and save to test-results.json
- Parse summary from JSON for logging

**Output format change**: Old format had `tests: [{name, status: "pass"|"fail"}]`, new format has `results: [{name, status: "passed"|"failed", duration_ms, message, file}]` plus nested `summary`. Downstream (s07_classify) doesn't consume test-results.json from s06 — it reads fixture outputs from step 7.

### 2. s04_clone_and_install.rs — Replace extract-tools with t3str extract

**Current**: `extract-tools tests '{clone_dir}' {lang_id}` → outputs JSON with test code bodies and fixtures

**New**: `t3str extract --language {lang_id} --repo-dir '{clone_dir}' --format json` → outputs JSON with test file paths and function names

**Changes**:
- Replace command string in `extract_test_data()`
- Remove fallback `echo '{"tests":[]}'` (t3str handles errors with structured JSON)
- Keep the same result-saving logic (write stdout to {lib_slug}-tests.json)

**Note**: Output format differs from extract-tools (no code bodies/fixtures — just function names). s05_generate_wrappers reads `-source.json` not `-tests.json`, so no downstream breakage.

### 3. config.rs — Minimal changes

- `lang_to_extractor_id()` stays — t3str uses the same language identifiers
- No new configuration needed (t3str is expected in PATH on the worker)

## Assumptions

- t3str binary is installed on the shedul3r worker machine (in PATH)
- Worker has the same language runtimes already verified by s04's verify_runtimes()

## Files Modified

| File | Repo | Change |
|------|------|--------|
| `tools/dev-process-v3/src/steps/s06_run_parsers.rs` | websmasher | Complete rewrite — remove Python script gen |
| `tools/dev-process-v3/src/steps/s04_clone_and_install.rs` | websmasher | Replace extract-tools call in extract_test_data() |
