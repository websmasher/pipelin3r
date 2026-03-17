# Add structured logging to run_agent

**Date:** 2026-03-17 21:20
**Scope:** packages/pipelin3r/src/executor/mod.rs, packages/pipelin3r/src/utils.rs

## Summary

Added structured tracing logs before and after every `run_agent` call so misconfigurations are immediately visible.

## Context & Problem

During real-world testing, two bugs were introduced in dev-process-v2:
1. `work_dir` set to language subdir instead of package root — agent couldn't see architecture.md
2. `tools` restricted to [Read, Write, WebSearch] instead of all tools — agent couldn't search exhaustively

Both bugs were invisible because run_agent was a black box — no logging of what config was being sent. The pipeline reported errors but didn't show WHY.

## Decision

Log the full AgentConfig (name, work_dir, tools, model, timeout, provider_id, max_concurrent, expect_outputs, prompt length) at INFO level before execution, and log the outcome (success/failure with output preview) after. This makes misconfigurations visible in the first run.

## Key Files

- `packages/pipelin3r/src/executor/mod.rs` — structured tracing::info/warn/error in run_agent
- `packages/pipelin3r/src/utils.rs` — added truncate_str utility for output preview
