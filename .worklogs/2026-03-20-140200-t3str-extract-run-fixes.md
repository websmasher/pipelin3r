# t3str Extract + Run Fixes (v0.1.3)

## Summary
Fixed test discovery (extract) gaps for 5 languages and test execution (run) for Go pre-modules repos, based on full pipeline validation against all 21 security-txt-parser libraries.

## Decisions

### Extract fixes
- **Python**: Added bare `test` stem match — catches `tests/test.py` pattern (pysecuritytxt)
- **Rust**: Scan all `src/` directory files for inline `#[cfg(test)]` modules — eikendev/sectxt has all tests inline in source files, no `tests/` dir
- **PHP**: `.phpt` files now match `*Test.phpt` (PascalCase) in addition to `test*.phpt` — spaze/security-txt uses PascalCase naming
- **Ruby**: Added `test "description"` DSL pattern to tree-sitter query — Rails `ActiveSupport::TestCase` style not just `it`/`def test_`
- **JavaScript**: Added bare `test`/`tests` stem match + `tests/` (plural) directory check — many repos use `test.js` at root or `tests/` dir

### Run fix
- **Go**: Auto-init `go.mod` when missing — runs `go mod init temp_module && go mod tidy` for pre-modules repos (tomnomnom, adamdecaf)

### Not fixed (out of scope)
- Ruby runner (rake output parsing) — needs minitest text parser, more complex
- PHP run failures — Nette Tester works correctly, tests genuinely fail

## Key files
- `apps/t3str/crates/adapters/outbound/extract/src/python.rs` — `is_test_file`
- `apps/t3str/crates/adapters/outbound/extract/src/rust_lang.rs` — `is_test_file` (scan src/)
- `apps/t3str/crates/adapters/outbound/extract/src/php.rs` — `is_test_file` (.phpt naming)
- `apps/t3str/crates/adapters/outbound/extract/src/ruby.rs` — QUERY (test DSL pattern)
- `apps/t3str/crates/adapters/outbound/extract/src/javascript.rs` — `is_test_file` (bare stem + tests/ dir)
- `apps/t3str/crates/adapters/outbound/run/src/go.rs` — go.mod init fallback
- `apps/t3str/Cargo.toml` — version bump to 0.1.3

## Next steps
- Deploy v0.1.3 to worker and re-run full test suite to validate fixes
- Consider adding minitest text parser for Ruby runner
- Consider adding Go GOPATH-style fallback for repos without go.mod that also lack network access
