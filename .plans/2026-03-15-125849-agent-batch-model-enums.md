# Add agent batch mode and typed Model/Provider enums to pipelin3r

**Date:** 2026-03-15 12:58
**Task:** Add batch execution support to AgentBuilder and typed Model/Provider enums

## Goal
1. AgentBuilder supports `.items(vec, concurrency).for_each(|item| AgentTask::new()...).execute().await` for batch execution via run_pool
2. Model and Provider enums replace raw strings for model selection, with provider-specific model ID mapping
3. lib.rs re-exports new types

## Approach

### Step-by-step plan

1. **Create `src/model.rs`** — Model enum (Opus4_6, Sonnet4_6, Haiku4_5, Custom), Provider enum (Anthropic, OpenRouter, Bedrock, Vertex, Custom), `Model::id(&self, &Provider) -> &str` with hardcoded mappings
2. **Create `AgentTask` struct in `src/agent.rs`** — carries prompt, working_dir, expected_output, bundle, auth override. Builder pattern with `new()` + chainable setters.
3. **Create `AgentBatchBuilder<T>` in `src/agent.rs`** — returned by `AgentBuilder::items()`. Has `for_each()` that takes closure mapping T -> AgentTask. `execute()` uses run_pool, inheriting model/timeout/tools from parent builder.
4. **Update `AgentBuilder::model()` to accept `Model` instead of `&str`** — store `Option<Model>` internally, convert to string via `Model::id()` when building task YAML
5. **Update `Executor` to hold optional default `Provider`** — add `with_default_provider()`, expose via getter
6. **Update `lib.rs` re-exports** — add model module, re-export Model, Provider, AgentTask
7. **Add tests** — Model::id for each provider, Custom pass-through, batch builder produces correct task count

### Key decisions
- **AgentTask is a standalone struct, not a builder** — it uses builder pattern (self-consuming setters) but is a data carrier. This keeps it simple and Send + 'static compatible for pool usage.
- **Model stores a Cow<'static, str> for Custom** — avoids lifetime issues while keeping zero-cost for known variants. Actually, use String for Custom to match existing patterns.
- **Provider on Executor, not AgentBuilder** — provider is typically set once for the whole pipeline. AgentBuilder inherits it.

## Files to Modify
- `packages/pipelin3r/src/model.rs` — NEW: Model + Provider enums
- `packages/pipelin3r/src/agent.rs` — Add AgentTask, AgentBatchBuilder, update model() signature
- `packages/pipelin3r/src/executor.rs` — Add default_provider field + setter + getter
- `packages/pipelin3r/src/task.rs` — No changes needed (still takes String for model)
- `packages/pipelin3r/src/lib.rs` — Add model module + re-exports
