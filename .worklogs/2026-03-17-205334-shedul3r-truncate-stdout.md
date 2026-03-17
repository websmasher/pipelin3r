# shedul3r: truncate subprocess stdout to 32KB

**Date:** 2026-03-17 20:53
**Scope:** apps/shedul3r/crates/app/commands/src/execute/mod.rs

## Summary

Capped subprocess stdout in TaskResponse to 32KB (tail). Prevents OOM and HTTP decode failures under load.

## Context & Problem

With 140 concurrent Claude Code sessions, shedul3r buffered megabytes of conversation log per task in memory, then serialized all of it into JSON responses. Clients got "error decoding response body" from reqwest because the responses were too large or malformed under memory pressure. The agents succeeded (files were written) but every HTTP response failed.

## Decision

- **Chose:** Truncate to last 32KB in `build_task_response`, keep the tail
- **Why:** The stdout from `claude -p` is a conversation log, not the work product. The real output is in files the agent wrote. 32KB of tail is enough for debugging.
- **Alternatives:** Cap in subprocess runner (rejected — the runner should capture everything for potential future use), stream response (rejected — overkill for this fix)

## Key Files

- `apps/shedul3r/crates/app/commands/src/execute/mod.rs` — `truncate_output()` function + `MAX_OUTPUT_BYTES` constant
