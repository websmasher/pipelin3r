# Fix remote upload: only declared inputs, not entire work_dir

**Date:** 2026-03-18 13:39
**Scope:** packages/pipelin3r/src/pipeline.rs

## Summary
PipelineContext now creates a temp dir with only the step's declared input files for remote execution, instead of uploading the entire base_dir. Fixes multipart upload failures when base dir has many files.
