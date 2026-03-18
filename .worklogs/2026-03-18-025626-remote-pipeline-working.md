# Remote pipeline execution working end-to-end

**Date:** 2026-03-18 02:56
**Scope:** shedul3r deploy, dev-process-v2 expect_outputs fix

## Summary

Remote execution of steps 1-2 via Railway-hosted shedul3r is fully working. 9/9 languages researched, files downloaded to local machine via bundle API.

## What was done

1. Released shedul3r 0.4.0 with async task API via GitHub Actions
2. Set NIXPACKS_START_CMD on Railway to install via cargo binstall + Claude Code
3. Fixed dev-process-v2 step 2 to set expect_outputs for file download
4. Disabled Cloudflare proxy on all claude-worker subdomains
5. Verified 9/9 research tasks complete with files downloaded

## Key Finding

The expect_outputs field must be set for remote execution — without it, agent-created files stay on the remote and aren't downloaded back. The path must be relative to work_dir.
