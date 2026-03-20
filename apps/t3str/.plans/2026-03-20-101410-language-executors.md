# Implement 3 language executor modules for t3str run adapter

**Date:** 2026-03-20 10:14
**Task:** Implement PHP, C#, and JavaScript executor modules replacing stubs

## Goal
Replace the stub implementations of `php.rs`, `csharp.rs`, and `javascript.rs` in the run adapter with real implementations that invoke language-native test runners and parse their output.

## Approach

### Step-by-step plan
1. Create `helpers.rs` with shared `build_summary`, `run_command`, `truncate_output` functions (if not created by another agent)
2. Add `mod helpers;` to `lib.rs`
3. Replace `php.rs` — detect Nette Tester vs PHPUnit, run appropriate command, parse output
4. Replace `csharp.rs` — run `dotnet test --verbosity normal`, parse stdout
5. Replace `javascript.rs` — try `npm test` + `npx jest --json`, parse with jest_json or mocha_text
6. Build and fix clippy issues

### Key decisions
- **Use `to_string_lossy()` for path conversion**: Paths might contain non-UTF8 chars, lossy conversion is safe for command args
- **All parsers return Vec, not Result for lenient parsing**: mocha_text, nette_text, dotnet_stdout return Vec directly; jest_json and junit_xml return Result

## Files to Modify
- `crates/adapters/outbound/run/src/helpers.rs` — new shared helpers
- `crates/adapters/outbound/run/src/lib.rs` — add `mod helpers`
- `crates/adapters/outbound/run/src/php.rs` — full PHP executor
- `crates/adapters/outbound/run/src/csharp.rs` — full C# executor
- `crates/adapters/outbound/run/src/javascript.rs` — full JS executor
