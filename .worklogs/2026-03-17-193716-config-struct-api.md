# pipelin3r API: Builder pattern → Config structs + functions

**Date:** 2026-03-17 19:37
**Scope:** packages/pipelin3r/ (agent, executor, pool, new modules: bundle_dir, utils)

## Summary

Replaced the builder/chaining API (AgentBuilder, AgentBatchBuilder) with config structs + plain functions. Added run_pool_map, BundleDir RAII utility, and text processing utilities.

## Context & Problem

The builder pattern (`.agent("x").model(M).prompt(p).execute()`) was fragile: forgotten `.execute()` silently drops work, intermediate state is invisible, hard to conditionally set fields. Config structs make everything explicit — you construct a struct, pass it to a function. Pipeline code is regular Rust with normal control flow between calls.

Additionally, AgentConfig was missing critical shedul3r scheduling fields (provider_id, max_concurrent, max_wait, retry) that every real pipeline uses.

## Decisions Made

### Config structs replace builders
- **Chose:** `AgentConfig::new(name, prompt)` + `executor.run_agent(&config)`
- **Why:** Explicit, inspectable, serializable, no "forgotten execute" footgun
- **Alternatives:** Keep builders (rejected — fragile), hybrid (rejected — two APIs is worse)

### AgentConfig::new() for required fields, no Default
- **Chose:** Constructor takes name and prompt, everything else is optional
- **Why:** Empty name/prompt from Default are traps — they compile but produce broken behavior
- **Alternatives:** Derive Default (rejected — footgun), Option<String> for name/prompt (rejected — adds unwrapping everywhere)

### run_pool_map closure returns (T, Result<R>)
- **Chose:** Item goes into the closure and comes back paired with result
- **Why:** Solves the ownership problem without Clone. Item identity preserved for correlation.
- **Alternatives:** Require T: Clone (rejected — wasteful for large items), return Vec<Result<R>> by index (rejected — loses item identity)

### Executor auto-forwards Claude env vars
- **Chose:** Read CLAUDE_ACCOUNT/CLAUDE_CONFIG_DIR at Executor construction, merge into every task
- **Why:** Every real pipeline needs this, making callers remember is error-prone
- **Alternatives:** Explicit env field only (rejected — 100% of callers would duplicate this)

### Tools as Vec<String> not enum
- **Chose:** `tools: Option<Vec<String>>` with raw tool names
- **Why:** Tool names are passed to CLI as `--allowedTools Read,Write`. An enum would need updating every time Claude Code adds a tool.
- **Alternatives:** Tool enum (rejected — maintenance burden, breaks on new tools)

## Key Files for Context

- `.plans/2026-03-17-191907-config-struct-api-v2.md` — the design plan with all adversarial findings addressed
- `packages/pipelin3r/src/agent/mod.rs` — AgentConfig, RetryConfig, AgentResult
- `packages/pipelin3r/src/executor/mod.rs` — Executor::run_agent(), auto_env
- `packages/pipelin3r/src/pool/mod.rs` — run_pool_map
- `packages/pipelin3r/src/bundle_dir.rs` — BundleDir RAII utility
- `packages/pipelin3r/src/utils.rs` — strip_code_fences, strip_preamble, parse_labeled_fields, chunk_by_size
- `.worklogs/2026-03-17-171052-pipelin3r-workdir-redesign.md` — prior worklog (work_dir foundation)

## Next Steps

1. Implement image generation module (OpenRouter API client, config struct)
2. Implement validate-and-fix function
3. Update README with new API examples
4. Consider CommandConfig as standalone function (currently CommandBuilder still exists)
