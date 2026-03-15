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
