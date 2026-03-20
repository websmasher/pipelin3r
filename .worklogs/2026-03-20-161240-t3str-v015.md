# t3str v0.1.4: Universal runner fixes for 100+ parser scale

## Summary
Fixed t3str runners to handle the long tail of project configurations across all 9 languages. These are universal patterns that will work for any future library, not one-off fixes for specific repos.

## Changes

### Python runner: retry on 0 collected tests
- When pytest collects 0 items (any exit code), retries with explicit file paths discovered via `find`
- Find pattern now includes bare `test.py` alongside `test_*.py` and `*_test.py`
- Fixes: projects with custom pytest config in pyproject.toml that restricts collection

### Rust runner: neutralize custom cargo configs
- Before `cargo test`, renames `.cargo/config.toml` and `.cargo/config` to `.bak`
- Restores after test completes (success or failure)
- Fixes: projects that set custom test runners (e.g., `runner = "sudo -E rlwrap"`) which interfere with output capture

### Go runner: detect actual module path
- When no `go.mod` exists, reads first `.go` file to extract `package <name>`
- Uses actual package name for `go mod init` instead of `temp_module`
- Falls back to directory name if no `.go` files found
- Fixes: pre-modules Go repos where `temp_module` breaks import resolution

### PHP runner: test directory discovery + text fallback
- When PHPUnit produces 0 results, retries with common test dirs (`tests/`, `test/`, `Test/`)
- When `--log-junit` isn't supported (old PHPUnit), falls back to text output parsing
- Text parser handles `OK (N tests, ...)` and `Tests: N, Failures: F, ...` formats
- Fixes: projects without phpunit.xml config or using legacy PHPUnit versions

### Nette Tester parser: complete rewrite
- Was looking for `-- PASSED:` format, actual format uses `√`/`×`/`s` Unicode markers
- Now captures all passing, failing, and skipped tests
- Parses failure detail messages and attaches to matching results

## Key files
- `apps/t3str/crates/adapters/outbound/run/src/python.rs`
- `apps/t3str/crates/adapters/outbound/run/src/rust_lang.rs`
- `apps/t3str/crates/adapters/outbound/run/src/go.rs`
- `apps/t3str/crates/adapters/outbound/run/src/php.rs`
- `apps/t3str/crates/adapters/outbound/run/src/parsers/nette_text.rs`
- `apps/t3str/crates/adapters/outbound/run/src/parsers/nette_text_tests.rs`

## Next steps
- Deploy v0.1.4 to worker, re-run full test suite
- The remaining failures (Ruby sqreen, Go tomnomnom) are genuine compatibility issues — sqreen needs Rails 5.1 on Ruby 2.x, tomnomnom fails to compile on Go 1.24 due to stricter format string rules
