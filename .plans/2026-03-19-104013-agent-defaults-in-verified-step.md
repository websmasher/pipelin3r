# Add agent_defaults to VerifiedStep

**Date:** 2026-03-19 10:40
**Task:** PromptedStep::resolve() creates bare AgentConfig with all Nones — no model, timeout, retry, provider. Must carry explicit defaults.

## Problem
`PromptedStep::resolve()` calls `AgentConfig::new(name, prompt)` which sets model=None, timeout=None, retry=None, provider=None. The orchestrator then does `AgentConfig { work_dir, expect_outputs, ..resolved.config }` — keeping all the Nones. This means every agent call goes out with no model, no timeout, no retry. We're relying on shedul3r to guess, which is magic.

## Fix
Add `agent_defaults: AgentConfig` to `VerifiedStep`. When the orchestrator resolves a PromptedStep, it merges the result with these defaults. The caller provides them once, all doer/breaker/fixer calls use them.

## Approach
1. Add `pub agent_defaults: AgentConfig` field to `VerifiedStep`
2. In orchestrator, after `step.doer.resolve()`, merge with defaults: `AgentConfig { name, prompt, work_dir, expect_outputs, ..step.agent_defaults }`
3. Same for fixer and breaker agents
4. Update `run_verified_step_batch` — the mapper closure returns VerifiedStep which already carries defaults
5. Update V3 main.rs to set agent_defaults on each VerifiedStep
6. Update tests

## Files to Modify
- `packages/pipelin3r/src/verified/mod.rs` — add field to VerifiedStep
- `packages/pipelin3r/src/verified/orchestrator.rs` — merge defaults on resolve
- `packages/pipelin3r/src/verified/tests.rs` — update test structs
- `websmasher/tools/dev-process-v3/src/main.rs` — set agent_defaults
