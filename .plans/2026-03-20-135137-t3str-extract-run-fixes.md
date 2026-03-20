# t3str Extract + Run Fixes

## Goal
Fix t3str test discovery and execution gaps found during full pipeline validation against 21 security-txt-parser libraries across 7 languages.

## Findings

Tested all 21 libraries. 8/21 work end-to-end. The remaining 13 have issues in extract (9), run (3), or both.

## Extract Fixes

### E1: Python — match bare `test` stem
**File**: `apps/t3str/crates/adapters/outbound/extract/src/python.rs`
**Why**: `tests/test.py` (pysecuritytxt) has stem `test` which doesn't match `test_*` or `*_test`.
**Fix**: Add `stem == "test"` to `is_test_file`.

### E2: Rust — scan source files for `#[cfg(test)]` modules
**File**: `apps/t3str/crates/adapters/outbound/extract/src/rust_lang.rs`
**Why**: eikendev/sectxt has all tests inline in `src/lib.rs`, `src/parsers.rs` etc. No `tests/` dir, no `test` in filenames.
**Fix**: Also scan `.rs` files not in `tests/` dir — check if file contains `#[cfg(test)]` and if so, include it.

### E3: PHP — match PascalCase `.phpt` files
**File**: `apps/t3str/crates/adapters/outbound/extract/src/php.rs`
**Why**: spaze uses `SecurityTxtParserTest.phpt` naming. Current check requires `starts_with("test")` for `.phpt`.
**Fix**: For `.phpt` files, also match `ends_with("Test")`.

### E4: Ruby — match `test "description"` DSL
**File**: `apps/t3str/crates/adapters/outbound/extract/src/ruby.rs`
**Why**: sqreen uses Rails `ActiveSupport::TestCase` with `test 'desc' do` blocks.
**Fix**: Add tree-sitter query pattern for `test` calls with string argument.

### E5: JavaScript — match bare `test` or `tests` stem
**File**: `apps/t3str/crates/adapters/outbound/extract/src/javascript.rs`
**Why**: `test.js` in root doesn't match `.test.` or `.spec.` pattern, not in `test/` dir.
**Fix**: Add `stem == "test" || stem == "tests"` to `is_test_file`.

### E6: JavaScript — also check `tests/` directory (with s)
**File**: `apps/t3str/crates/adapters/outbound/extract/src/javascript.rs`
**Why**: Some projects use `tests/` (plural) not `test/` (singular).
**Fix**: Add `"tests"` to the directory name check.

## Run Fixes

### R1: Go — init go.mod for pre-modules repos
**File**: `apps/t3str/crates/adapters/outbound/run/src/go.rs`
**Why**: tomnomnom/securitytxt and adamdecaf/go-security-txt have no `go.mod`. `go test ./...` fails.
**Fix**: Before running `go test`, check if `go.mod` exists. If not, run `go mod init temp && go mod tidy` first.

### R2: Ruby — parse minitest output from rake
**File**: `apps/t3str/crates/adapters/outbound/run/src/ruby.rs`
**Why**: Rake test output is captured but never parsed. Minitest output follows a predictable format.
**Fix**: Add minitest text parser. Also: try `bundle exec ruby -Itest -e "Dir['test/**/*_test.rb'].each{|f| require \"./#{f}\"}"` as alternative to rake.

## Not Fixing (No Tests / Real Failures)

- Python `securitytxt-parser`: No test files exist in repo
- Go `diosts`: No test files in project source
- PHP `spaze` run failures: Real test failures (Nette Tester works correctly)
- Rust `secmap`: Complex workspace with external deps, 0 tests found because tests are in cargo-home deps
- JS `security.txt-extension`: Browser extension, no test suite

## Files to Modify

1. `apps/t3str/crates/adapters/outbound/extract/src/python.rs` — E1
2. `apps/t3str/crates/adapters/outbound/extract/src/rust_lang.rs` — E2
3. `apps/t3str/crates/adapters/outbound/extract/src/php.rs` — E3
4. `apps/t3str/crates/adapters/outbound/extract/src/ruby.rs` — E4
5. `apps/t3str/crates/adapters/outbound/extract/src/javascript.rs` — E5, E6
6. `apps/t3str/crates/adapters/outbound/run/src/go.rs` — R1
7. `apps/t3str/crates/adapters/outbound/run/src/ruby.rs` — R2 (stretch)
