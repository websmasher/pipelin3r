# Add CommandConfig, validate_and_fix, image_gen modules

**Date:** 2026-03-17 20:08
**Scope:** packages/pipelin3r/ — command, validate, image_gen modules

## Summary

Completed the config-struct API migration and added two new modules: validate_and_fix for validation-remediation loops, and image_gen for OpenRouter image generation.

## Changes

### CommandConfig (replace CommandBuilder)
- `CommandConfig::new(program)` + `run_command(&config)`
- Added `env` and `timeout` fields
- Tests for env passthrough and timeout behavior

### validate_and_fix
- `ValidateConfig`, `ValidateResult`, `ValidationReport`, `ValidationFinding`, `RemediationAction`
- Three remediation strategies: AgentFix (LLM), FunctionFix (programmatic), Skip
- Selective per-item remediation via strategy closure
- Tag-based finding routing (findings_with_tag)
- `to_markdown()` for prompt inclusion
- `require_converged()` for error conversion
- 18 tests

### image_gen
- `ImageGenHttpConfig` with API key + rate limiting via limit3r
- `ImageGenConfig` with model, prompt, reference images, aspect ratio
- `generate_image()` standalone function calling OpenRouter API directly
- Base64 image decoding, cost tracking, follow-up cost lookup
- RefImage with roles (Style, CharSheet, Input) and file loading
- AspectRatio/ImageModel enums with OpenRouter API string mappings
- 15 tests

## Key Files for Context

- `packages/pipelin3r/src/command/mod.rs` — CommandConfig + run_command
- `packages/pipelin3r/src/validate/mod.rs` — validate_and_fix loop
- `packages/pipelin3r/src/validate/report.rs` — ValidationReport types
- `packages/pipelin3r/src/validate/action.rs` — RemediationAction enum
- `packages/pipelin3r/src/image_gen/mod.rs` — generate_image function
- `packages/pipelin3r/src/image_gen/client.rs` — OpenRouter HTTP client
- `packages/pipelin3r/src/image_gen/types.rs` — AspectRatio, ImageModel, RefImage
