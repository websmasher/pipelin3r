# Replace hardcoded model IDs with TOML-based config

**Date:** 2026-03-15 13:24
**Task:** Move model ID resolution from hardcoded match arms to TOML config, with embedded defaults.

## Goal
Model IDs come from a TOML config file. Hardcoded values become fallback defaults. Users can override by loading custom TOML.

## Approach

### Step-by-step plan
1. Create `packages/pipelin3r/models.toml` with per-provider model IDs
2. Add `toml` and `serde` dependencies to pipelin3r Cargo.toml
3. Add `config_key()` methods to `Model` and `Provider` enums in model.rs
4. Add `ModelConfig` struct with `from_toml`, `from_file`, `default_config`, `resolve` methods
5. Embed models.toml via `include_str!` as compile-time default
6. Add `model_config` field to `Executor`, with `with_model_config()` builder
7. Add `model_config()` accessor to Executor
8. Update `resolve_model_string` in agent.rs to use executor's model_config
9. Export `ModelConfig` from lib.rs
10. Write tests for ModelConfig

### Key decisions
- **Use `serde::Deserialize` for TOML parsing:** The TOML structure maps directly to `BTreeMap<String, BTreeMap<String, String>>` which serde handles natively.
- **Keep `Model::id()` working as-is:** Existing hardcoded method remains for backward compat; `ModelConfig::resolve` is the new preferred path.
- **`serde` as full dep, not dev-dep:** Needed at runtime for loading custom TOML configs.

## Files to Modify
- `packages/pipelin3r/models.toml` — new, TOML config
- `packages/pipelin3r/Cargo.toml` — add toml + serde deps
- `packages/pipelin3r/src/model.rs` — add config_key methods, ModelConfig struct + tests
- `packages/pipelin3r/src/executor.rs` — add model_config field + builder
- `packages/pipelin3r/src/agent.rs` — use model_config.resolve()
- `packages/pipelin3r/src/lib.rs` — export ModelConfig
