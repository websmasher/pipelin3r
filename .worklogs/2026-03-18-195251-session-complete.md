# Session complete: pipelin3r end-to-end validated

**Date:** 2026-03-18 19:52
**Scope:** Full session — pipelin3r design, implementation, testing, deployment, validation

## What was built

### pipelin3r library
- Config-struct API (AgentConfig, CommandConfig, ImageGenConfig, ValidateConfig)
- PipelineContext for step orchestration with input/output file routing
- Remote execution via temp dirs (only declared inputs uploaded)
- Async polling for Railway/proxy compatibility
- 490+ tests including adversarial rounds
- Utilities: strip_code_fences, strip_preamble, parse_labeled_fields, chunk_by_size, BundleDir

### shedul3r server
- Async task API (POST /api/tasks/async, GET /api/tasks/async/{id})
- Stdout truncation (32KB)
- Actix timeout fix (1hr keep-alive)
- Content-Disposition filename fix for multipart uploads with paths

### shedul3r SDK
- ApiElapsed float/struct deserialization
- submit_task_poll with timeout + transient retry
- Raw bytes parsing for better error diagnostics

### dev-process-v2
- 6 steps implemented and tested (scaffold, research, extract libraries, clone+tests, clone+source, filter tests)
- PipelineContext-based with fail-stop
- Works locally and remotely through Railway

## Bugs found and fixed (total across session)
1. ApiElapsed deserialization mismatch (float vs struct)
2. CLAUDE_CONFIG_DIR breaks remote execution
3. Empty work_dir sends local path to remote
4. Download runs on failed tasks
5. Actix-web default timeouts (5s)
6. Stdout truncation needed
7. Multipart part names with slashes rejected by actix
8. File-poll recovery not wired in
9. Work_dir set to wrong directory (lang subdir vs package root)
10. Tools over-restricted (Read/Write/WebSearch vs all)
11. Cloudflare proxy timeouts (524)

## Key architectural decisions
- Config structs + functions, not builders/chains
- PipelineContext creates temp dirs with only declared inputs for remote
- Programmatic steps (tree-sitter) run locally, not through shedul3r
- Pipeline steps declare inputs/outputs, context handles transport

## V1 vs V2 comparison
- V1 fresh run finds ~32 repos (including generators)
- V2 finds ~17 repos (parser-only, filtered at step 3)
- When comparing parser-only: coverage is comparable
- V2 step 3 added premature filtering not present in v1 — should be removed for correct v1 parity
