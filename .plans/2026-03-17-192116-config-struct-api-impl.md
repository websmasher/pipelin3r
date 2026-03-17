# Implement config-struct API redesign for agent execution

**Date:** 2026-03-17 19:21
**Task:** Replace AgentBuilder/AgentBatchBuilder/AgentTask with AgentConfig + run_agent on Executor

## Goal
Replace the builder pattern in agent/mod.rs with a config struct pattern: `AgentConfig` + `Executor::run_agent()`. Remove batch-specific infrastructure from agent module (batch will use `run_pool_map` in a future PR). Keep all working infrastructure: dry-run capture, work-dir transport, execute helpers.

## Approach

### Step-by-step plan

1. **agent/mod.rs** — Replace AgentBuilder, AgentBatchBuilder, AgentTask, BatchConfig with:
   - `AgentConfig` struct (name, prompt required; model, work_dir, execution_timeout, tools, auth, env, provider_id, max_concurrent, max_wait, retry, expect_outputs, request_timeout optional)
   - `RetryConfig` struct
   - Updated `AgentResult` with `output_files: BTreeMap<String, String>`
   - Keep `AgentResult::require_success()`

2. **executor/mod.rs** — Add:
   - `auto_env: BTreeMap<String, String>` field
   - `capture_claude_env()` helper
   - `pub async fn run_agent(&self, config: &AgentConfig)` method
   - Remove `agent()`, `command()`, `transform()` factory methods

3. **agent/execute.rs** — Adapt:
   - Remove `execute_single_task`, `execute_batch_task_dry_run`, `count_batch_outcomes`, `is_partial_failure`
   - Update `execute_dry_run_capture` to return updated `AgentResult` with empty output_files
   - Update `execute_with_work_dir` to read expect_outputs into BTreeMap after execution
   - Keep `validate_work_dir`, `format_duration`, relative path helpers

4. **task/mod.rs** — Update `TaskConfig` to accept retry config fields (initial_delay, backoff_multiplier, max_delay as optional strings)

5. **lib.rs** — Update exports: remove AgentBuilder, AgentTask; add AgentConfig, RetryConfig

6. **tests** — Rewrite agent/tests.rs and executor/tests.rs; update integration tests

## Files to Modify
- `packages/pipelin3r/src/agent/mod.rs`
- `packages/pipelin3r/src/agent/execute.rs`
- `packages/pipelin3r/src/agent/tests.rs`
- `packages/pipelin3r/src/executor/mod.rs`
- `packages/pipelin3r/src/executor/tests.rs`
- `packages/pipelin3r/src/task/mod.rs`
- `packages/pipelin3r/src/task/tests.rs`
- `packages/pipelin3r/src/lib.rs`
- `packages/pipelin3r/tests/integration.rs`
