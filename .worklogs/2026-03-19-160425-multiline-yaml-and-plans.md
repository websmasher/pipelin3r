# Fix multiline command in task YAML + worker Dockerfile plans

**Date:** 2026-03-19 16:04
**Scope:** packages/pipelin3r/src/task/mod.rs

## Summary
Fixed task YAML builder to support multiline shell commands using YAML literal block scalar (`|`). Added plans for worker Dockerfile with all language runtimes.

## Context & Problem
Remote shell commands submitted via `run_remote_command` can be multi-line scripts. The task YAML builder wrote `command: <value>` on a single line, breaking YAML parsing when the command contained newlines.

## Decisions Made
- **Chose:** Use YAML literal block scalar (`command: |`) when command contains newlines, plain `command: value` otherwise
- **Why:** serde_yaml on the shedul3r side already supports block scalars. No server changes needed.
