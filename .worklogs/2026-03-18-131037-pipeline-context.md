# Add PipelineContext for step orchestration

**Date:** 2026-03-18 13:10
**Scope:** packages/pipelin3r/src/pipeline.rs, dev-process-v2 rewrite

## Summary

Added PipelineContext that manages input/output file routing between pipeline steps. Steps declare inputs (files they read) and outputs (files they produce). The context handles transport — local paths or remote bundle upload/download.

## Key decisions

- inputs/outputs are Vec<String> relative paths from base_dir
- For remote: only inputs are uploaded (not whole dir), only outputs downloaded back
- run_local for programmatic steps — no transport, just filesystem access
- Pipeline fail-stops on any step failure
- Timeout increased to 30 min for remote agents
