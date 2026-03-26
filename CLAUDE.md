# pipelin3r — Agent Instructions

> **This is an agent-managed codebase.** The user does not read or write code directly. Your role is to own the code end-to-end. Never say "you can edit X" — just do it. Never estimate workload — just execute.

## What This Project Does

Rust monorepo for LLM pipeline orchestration. Four packages provide resilience patterns, task scheduling, and pipeline workflow execution. The `shedul3r` server is the deployable backend that executes scheduled tasks. All packages will be published to crates.io.

## Architecture Overview

```
pipelin3r/
├── Cargo.toml                     # Root workspace for published packages
├── deny.toml                      # Dependency audit (cargo-deny) for packages
│
├── packages/                      # Published crates (crates.io)
│   ├── limit3r/                   # Resilience patterns library
│   ├── shedul3r-rs-sdk/           # Rust client SDK for shedul3r server
│   └── pipelin3r/                 # Pipeline orchestration library
│
├── apps/
│   └── shedul3r/                  # Deployable task execution server (Axum)
│       ├── Cargo.toml             # Separate Cargo workspace
│       ├── deny.toml              # Dependency audit for server
│       ├── clippy.toml            # Clippy configuration
│       └── crates/                # Hexagonal architecture crates
│
└── golden-tests/                  # End-to-end golden file tests
    ├── compare.sh                 # Comparison runner
    ├── run-golden.sh              # Test execution script
    ├── fixtures/                  # Input fixtures
    └── golden/                    # Expected output files
```

## What Each Package Does

| Package | Description |
|---------|------------|
| **limit3r** | In-memory resilience patterns: rate limiter, circuit breaker, bulkhead, retry. Zero external service dependencies. |
| **shedul3r-rs-sdk** | Rust client SDK for the shedul3r task execution server. Communicates via HTTP/REST using reqwest. |
| **pipelin3r** | Pipeline orchestration for LLM-powered workflows. Composes steps into directed graphs with scheduling via shedul3r. |
| **shedul3r** (app) | Axum HTTP server for task scheduling and execution. Hexagonal architecture with SQLx/Postgres. Deployed via Railway. |

## Dependency Chain

```
pipelin3r → shedul3r-rs-sdk → (HTTP) → shedul3r → limit3r
```

- `pipelin3r` depends on `shedul3r-rs-sdk` (Cargo path dependency)
- `shedul3r-rs-sdk` communicates with `shedul3r` server over HTTP (no Cargo dependency)
- `shedul3r` server depends on `limit3r` for rate limiting and resilience (Cargo path dependency in its own workspace)

## Two Workspaces

This repo contains **two separate Cargo workspaces**:

1. **Root workspace** (`/Cargo.toml`) — the three published packages (`limit3r`, `shedul3r-rs-sdk`, `pipelin3r`). This is what `cargo build` at the root builds.
2. **Server workspace** (`/apps/shedul3r/Cargo.toml`) — the shedul3r application with its hexagonal architecture crates. This is a separate workspace excluded from the root.

Commands must target the correct workspace. See build/test instructions below.

## How to Build

```bash
# Packages (from repo root)
cargo build

# shedul3r server
cd apps/shedul3r && cargo build

# Release build (server)
cd apps/shedul3r && cargo build --release
```

## How to Test

```bash
# Package tests (from repo root)
cargo test --workspace

# shedul3r server tests
cd apps/shedul3r && cargo test --workspace

# Golden tests (requires release build of shedul3r)
cd apps/shedul3r && cargo build --release
bash golden-tests/compare.sh apps/shedul3r/target/release/shedul3r
```

## How to Lint

```bash
# Packages
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check

# shedul3r server
cd apps/shedul3r && cargo fmt --all -- --check
cd apps/shedul3r && cargo clippy --workspace --all-targets -- -D warnings
cd apps/shedul3r && cargo deny check
```

## Deployment

- **shedul3r** is deployed via Railway (has `railpack-shedul3r.json` config)
- Push to `main` triggers auto-deploy
- Do NOT run `railway up` — it bypasses git history

## Publishing

All three packages (`limit3r`, `shedul3r-rs-sdk`, `pipelin3r`) are intended for publication to **crates.io**. Each has:
- `license = "MIT"`
- `repository` pointing to the GitHub repo
- `description`, `keywords`, `categories` metadata
- `README.md` in the package directory

When publishing, respect the dependency order: `limit3r` first (no internal deps), then `shedul3r-rs-sdk`, then `pipelin3r`.

## Workspace Lint Configuration

Both workspaces enforce identical strict Clippy and rustc lints. Key settings from the root `Cargo.toml`:

- `unsafe_code = "forbid"` — no unsafe anywhere
- `missing_docs = "deny"` — all public items need doc comments
- All Clippy groups (`all`, `pedantic`, `cargo`, `nursery`) at `deny` level
- Panic-inducing patterns banned: `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice`, `arithmetic_side_effects`
- Output discipline: `dbg_macro`, `print_stdout`, `print_stderr` denied
- Type safety: `as_conversions`, `float_cmp` denied

See the full lint table in `/Cargo.toml` `[workspace.lints.clippy]`.

## Don'ts

1. **Don't use `unwrap()` or `expect()`.** Use `?` with proper error types, or `.ok_or_else()` / `.map_err()`. Clippy enforces this.
2. **Don't use `unsafe`.** It's `forbid` at the workspace level.
3. **Don't index directly.** `vec[i]` panics — use `.get(i)` and handle `None`. Clippy `indexing_slicing` enforces this.
4. **Don't slice strings directly.** `&s[0..5]` panics on non-ASCII — use `.get(0..5)` or `.chars()`. Clippy `string_slice` enforces this.
5. **Don't use bare arithmetic.** `a + b` can overflow — use `.checked_add()` / `.saturating_add()`. Clippy `arithmetic_side_effects` enforces this.
6. **Don't use `anyhow` in published packages.** Use `thiserror` for typed error enums. `anyhow` erases error types and prevents callers from matching variants.
7. **Don't use `HashMap` or `HashSet`.** Use `BTreeMap`/`BTreeSet` for deterministic ordering.
8. **Don't use `chrono`.** Use the `time` crate.
9. **Don't use `tokio` with `features = ["full"]`.** List features explicitly to keep binary size down.
10. **Don't add `#[allow(...)]` without a justification comment.** Every item-level `#[allow]` must have a `//` comment on the same line explaining why.
11. **Don't relax guardrail configs.** Changing `"deny"` to `"warn"`/`"allow"` in Cargo.toml or deny.toml is not permitted without `# EXCEPTION: reason`.
12. **Don't create files over 500 effective lines.** Split into modules.
13. **Don't cross workspace boundaries.** Root packages cannot import shedul3r server crates. The server has its own workspace.
14. **Don't skip `cargo deny check`.** CI enforces it on both workspaces.

## shedul3r Server Details

The shedul3r server follows hexagonal architecture. See `apps/shedul3r/CLAUDE.md` for server-specific instructions including crate hierarchy, dependency rules, error handling patterns, and per-layer clippy.toml enforcement.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **pipelin3r** (2376 symbols, 5537 relationships, 177 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## When Debugging

1. `gitnexus_query({query: "<error or symptom>"})` — find execution flows related to the issue
2. `gitnexus_context({name: "<suspect function>"})` — see all callers, callees, and process participation
3. `READ gitnexus://repo/pipelin3r/process/{processName}` — trace the full execution flow step by step
4. For regressions: `gitnexus_detect_changes({scope: "compare", base_ref: "main"})` — see what your branch changed

## When Refactoring

- **Renaming**: MUST use `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` first. Review the preview — graph edits are safe, text_search edits need manual review. Then run with `dry_run: false`.
- **Extracting/Splitting**: MUST run `gitnexus_context({name: "target"})` to see all incoming/outgoing refs, then `gitnexus_impact({target: "target", direction: "upstream"})` to find all external callers before moving code.
- After any refactor: run `gitnexus_detect_changes({scope: "all"})` to verify only expected files changed.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Tools Quick Reference

| Tool | When to use | Command |
|------|-------------|---------|
| `query` | Find code by concept | `gitnexus_query({query: "auth validation"})` |
| `context` | 360-degree view of one symbol | `gitnexus_context({name: "validateUser"})` |
| `impact` | Blast radius before editing | `gitnexus_impact({target: "X", direction: "upstream"})` |
| `detect_changes` | Pre-commit scope check | `gitnexus_detect_changes({scope: "staged"})` |
| `rename` | Safe multi-file rename | `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` |
| `cypher` | Custom graph queries | `gitnexus_cypher({query: "MATCH ..."})` |

## Impact Risk Levels

| Depth | Meaning | Action |
|-------|---------|--------|
| d=1 | WILL BREAK — direct callers/importers | MUST update these |
| d=2 | LIKELY AFFECTED — indirect deps | Should test |
| d=3 | MAY NEED TESTING — transitive | Test if critical path |

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/pipelin3r/context` | Codebase overview, check index freshness |
| `gitnexus://repo/pipelin3r/clusters` | All functional areas |
| `gitnexus://repo/pipelin3r/processes` | All execution flows |
| `gitnexus://repo/pipelin3r/process/{name}` | Step-by-step execution trace |

## Self-Check Before Finishing

Before completing any code modification task, verify:
1. `gitnexus_impact` was run for all modified symbols
2. No HIGH/CRITICAL risk warnings were ignored
3. `gitnexus_detect_changes()` confirms changes match expected scope
4. All d=1 (WILL BREAK) dependents were updated

## CLI

- Re-index: `npx gitnexus analyze`
- Check freshness: `npx gitnexus status`
- Generate docs: `npx gitnexus wiki`

<!-- gitnexus:end -->
