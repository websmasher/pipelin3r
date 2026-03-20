# Implement 3 language executor modules for the t3str run adapter

**Date:** 2026-03-20 10:13
**Task:** Implement Python, Go, and Rust executor modules plus shared helpers

## Goal
Replace the stub executors for Python, Go, and Rust with real implementations that build commands, execute via tokio::process::Command, parse output with existing parsers, and return TestSuite.

## Approach

### Step-by-step plan
1. Create `helpers.rs` with `build_summary`, `run_command`, `truncate_output`
2. Add `mod helpers;` to `lib.rs`, remove unused dep markers
3. Replace `python.rs` stub with pytest executor using junit_xml parser
4. Replace `go.rs` stub with `go test -json` executor using go_json parser
5. Replace `rust_lang.rs` stub with `cargo test` executor using cargo_text parser

### Key decisions
- **Shared helpers module**: Extract common logic (summary building, command execution, output truncation) to avoid duplication across 9 language modules
- **tokio::process::Command**: Not banned by clippy.toml (only std::process::Command::new is noted as allowed)
- **Timeout per language**: Python/Go 300s, Rust 600s (compilation time)

## Files to Modify
- `crates/adapters/outbound/run/src/helpers.rs` — new shared helpers
- `crates/adapters/outbound/run/src/lib.rs` — add `mod helpers`, remove unused dep markers
- `crates/adapters/outbound/run/src/python.rs` — real pytest executor
- `crates/adapters/outbound/run/src/go.rs` — real go test executor
- `crates/adapters/outbound/run/src/rust_lang.rs` — real cargo test executor
