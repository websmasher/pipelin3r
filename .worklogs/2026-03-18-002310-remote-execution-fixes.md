# Remote execution fixes + real-world testing

**Date:** 2026-03-18 00:23
**Scope:** packages/pipelin3r/src/agent/execute.rs, packages/pipelin3r/src/executor/mod.rs, packages/shedul3r-rs-sdk/src/client/mod.rs

## Summary

Fixed 3 bugs discovered during remote execution testing against Railway-hosted shedul3r.

## Fixes

### 1. Empty work_dir sends local path to remote
When work_dir had no files, bundle upload was skipped, sending the local path to the remote server. Fixed: always upload a bundle for remote, add `.gitkeep` placeholder when empty.

### 2. Download runs on failed tasks
Expected output download ran unconditionally after submit_task. If the task failed, download returned 404, masking the real error. Fixed: gate download on `result.success`.

### 3. CLAUDE_CONFIG_DIR breaks remote execution
Auto-forwarded local `CLAUDE_CONFIG_DIR` path doesn't exist on remote machine. Claude Code fails silently (exits 0 in 1.4s, no output). Fixed: exclude `CLAUDE_CONFIG_DIR` from auto_env when `is_local()` returns false.

## Remaining Issue

Remote step 2 fails with Cloudflare 524 timeout — Cloudflare's proxy kills connections after ~2 minutes, but agent tasks take 5-15 minutes. This is an infrastructure issue (Cloudflare timeout config or need for async task API), not a pipelin3r bug.

## Key Files

- `packages/pipelin3r/src/agent/execute.rs` — empty bundle fix, download gate, debug logging
- `packages/pipelin3r/src/executor/mod.rs` — CLAUDE_CONFIG_DIR exclusion for remote
