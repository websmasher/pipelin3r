# Write PipelineContext integration tests

**Date:** 2026-03-18 14:05
**Task:** Write tests for PipelineContext in a new test file

## Goal
Create `/packages/pipelin3r/tests/pipeline_context.rs` covering input verification, output verification, run_local, remote temp dir behavior, and AgentStep construction. All tests use dry-run mode.

## Approach
Single new test file with 13 test functions. Follow patterns from `integration.rs` (allow attrs, unused dep suppression, tempfile usage). Use `Executor::with_defaults()?.with_dry_run()` for local tests and `Executor::new(&ClientConfig{base_url: remote})?.with_dry_run()` for remote tests.

## Files to Modify
- `packages/pipelin3r/tests/pipeline_context.rs` — new file with all tests
