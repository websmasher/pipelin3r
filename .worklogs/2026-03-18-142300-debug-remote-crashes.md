# Debug remote agent crashes — root cause found

**Date:** 2026-03-18 14:23
**Scope:** packages/pipelin3r/src/pipeline.rs, tests/pipeline_context.rs

## Summary

The "Exit 1: " crashes during remote execution were NOT resource pressure. They were caused by the multipart upload bug — uploading the entire base_dir failed for large directories, giving agents broken work dirs.

## Root Cause

When PipelineContext set `work_dir` to the package root (with 30+ files from previous steps), the multipart upload to shedul3r failed with "Content-Disposition header not found." The agent got an empty temp dir on the remote, couldn't find input files, and exited with code 1. The empty stderr was because Claude Code exits cleanly (no error message) when it has no files to work with.

## Evidence

- Railway container has 256GB RAM, 32 CPUs — not resource-limited
- Single agent runs succeed (work_dir is small)
- 5 concurrent research agents succeed when each uploads only 0-1 input files
- All previous failures correlated with large work_dir uploads

## Fix

PipelineContext now creates a temp dir per agent with only the declared `inputs` files, instead of uploading the entire base_dir. This keeps each upload small (1-2 files) regardless of how much data previous steps produced.

## Tests

13 PipelineContext tests added covering input verification, output checking, local execution, and remote temp dir behavior.
