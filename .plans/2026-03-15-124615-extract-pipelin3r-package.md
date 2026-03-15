# Extract pipelin3r pipeline orchestration package

**Date:** 2026-03-15 12:46
**Task:** Port dev-process template/task/schedulr/pool code into the pipelin3r package with the new API design

## Goal
pipelin3r package has fully working auth, template, executor, agent, bundle, command, and transform modules. `cargo check` and `cargo test -p pipelin3r` pass.

## Approach

### Files to create/modify
1. `src/auth.rs` — Auth enum with to_env(), from dev-process schedulr.rs claude_env + merge_env
2. `src/template.rs` — Direct port of TemplateFiller from dev-process template.rs
3. `src/task.rs` (new private module) — Port build_task_yaml from dev-process task.rs
4. `src/executor.rs` — Wraps SDK Client + Auth + DryRun, ports dry-run from schedulr.rs
5. `src/agent.rs` — AgentBuilder with execute(), ports task building + dry-run capture
6. `src/bundle.rs` — Bundle builder (local temp dir for now)
7. `src/command.rs` — Shell command wrapper via tokio::process::Command
8. `src/transform.rs` — Stub
9. `src/pool.rs` (new) — Port bounded concurrency from dev-process pool.rs
10. `src/lib.rs` — Re-exports + module declarations

### Key decisions
- Task YAML building is a private module (not re-exported) used by agent.rs
- Pool is a public module for batch execution
- DryRun state is per-Executor instance (not global static like in dev-process)
- extract_step_name ported into executor/agent as private helper
