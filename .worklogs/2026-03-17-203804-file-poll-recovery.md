# Wire file-poll recovery into run_agent

**Date:** 2026-03-17 20:38
**Scope:** packages/pipelin3r/src/agent/execute.rs

## Summary

Fixed a bug where run_agent used submit_task (plain HTTP) instead of submit_task_with_recovery (HTTP raced against file polling) for local execution with expected outputs. shedul3r drops HTTP connections for long-running tasks, but the agent still completes and writes output files. The SDK's recovery mechanism catches this by polling for file existence.

## Context & Problem

During real-world testing (dev-process-v2 running steps 1-2 against websmasher-security-txt-parser-v2), every agent call failed with "error decoding response body" from reqwest. The agents actually succeeded — they wrote their output files correctly — but the HTTP response from shedul3r was malformed or truncated (137 active tasks, server under load).

The shedul3r-rs-sdk has `submit_task_with_recovery` which races the HTTP call against file polling. This was NOT wired into pipelin3r's `execute_with_work_dir` — it always used `submit_task` (plain HTTP, no recovery).

## Decision

When local (shared filesystem) AND `expect_outputs` is non-empty, use `submit_task_with_recovery` with the first expected output file as the poll target. When remote or no expected outputs, use `submit_task` as before.

## Key Files

- `packages/pipelin3r/src/agent/execute.rs` — the fix
- `packages/shedul3r-rs-sdk/src/client/mod.rs` — submit_task_with_recovery (already existed)
