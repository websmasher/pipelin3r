# Implement t3str discovery port and commands crates

**Date:** 2026-03-20 09:43
**Task:** Replace stubs in t3str-discovery-port and t3str-commands with actual implementations

## Goal
Two crates fully implemented with port traits and application commands, plus clippy.toml for each.

## Approach

### Step-by-step plan
1. Write `crates/ports/outbound/discovery/src/lib.rs` with `TestDiscoverer` (sync) and `TestExecutor` (async) traits
2. Write `crates/ports/outbound/discovery/clippy.toml` — ports layer bans (no I/O, no process, no env)
3. Write `crates/app/commands/src/lib.rs` as module root
4. Write `crates/app/commands/src/extract.rs` — `ExtractCommand` delegating to `TestDiscoverer`
5. Write `crates/app/commands/src/run.rs` — `RunCommand` delegating to `TestExecutor`
6. Write `crates/app/commands/clippy.toml` — same bans as ports layer
7. Build and fix clippy issues

## Files to Modify
- `apps/t3str/crates/ports/outbound/discovery/src/lib.rs`
- `apps/t3str/crates/ports/outbound/discovery/clippy.toml` (new)
- `apps/t3str/crates/app/commands/src/lib.rs`
- `apps/t3str/crates/app/commands/src/extract.rs` (new)
- `apps/t3str/crates/app/commands/src/run.rs` (new)
- `apps/t3str/crates/app/commands/clippy.toml` (new)
