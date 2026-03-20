# shedul3r v0.5.2: increase output limit to 1MB

## Summary
Increased MAX_OUTPUT_BYTES from 32KB to 1MB. DomainDetective's test suite produces 632KB of JSON output which was being truncated, resulting in invalid JSON in the pipeline's ground-truth files.

## Key files
- `apps/shedul3r/crates/app/commands/src/execute/mod.rs` — MAX_OUTPUT_BYTES
