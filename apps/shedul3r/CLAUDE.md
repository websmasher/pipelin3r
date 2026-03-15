# Rust Backend — Agent Instructions

> **This is a TEMPLATE, not a live application.** The sample code (health endpoint, error types, extractors) exists as a reference pattern. When starting a real project, add your domain modules following the hexagonal architecture below.

## What This Service Is

Rust/Axum HTTP backend for business logic, scheduling, and background jobs. Communicates with the Next.js web app via HTTP over Railway's private network. Owns its own database access via SQLx.

## Hexagonal Architecture — Required

Hexagonal architecture isolates business logic from infrastructure. Each layer is independently modifiable — agent mistakes can't cascade across boundaries. Every module follows this pattern.

### Crate Hierarchy

```
crates/
├── domain/
│   └── types/              # Crate: domain-types
│                           # Entities, value objects, domain errors
│                           # ZERO internal dependencies. Pure data + logic.
│
├── ports/
│   └── outbound/
│       └── repo/           # Crate: repo
│                           # Trait definitions (repository interfaces)
│                           # Depends only on domain-types
│
├── app/
│   └── commands/           # Crate: commands
│                           # Use cases / business logic
│                           # Depends on domain-types + repo only
│                           # Receives adapters via trait bounds — never imports them
│
└── adapters/
    ├── inbound/
    │   └── api/            # Crate: api
    │                       # Composition root — Axum handlers + main.rs
    │                       # The ONLY crate that wires adapters to app layer
    │                       # Depends on everything
    │
    └── outbound/
        └── db/             # Crate: db
                            # Concrete implementations (SQLx)
                            # Depends on domain-types + repo
```

### Dependency Rules

| Crate | May depend on | Must NOT depend on |
|-------|--------------|-------------------|
| domain-types | (nothing internal) | repo, commands, db, api |
| repo | domain-types | commands, db, api |
| commands | domain-types, repo | db, api |
| db | domain-types, repo | commands (except dev-deps for tests), api |
| api | everything | — (composition root) |

These rules are enforced by Cargo's dependency graph. If a crate's `Cargo.toml` doesn't list a dependency, it can't import it.

### Layer Responsibilities

**Domain Types** (`crates/domain/types/`, crate: `domain-types`)
- Entities, value objects, enums, domain errors. Zero internal dependencies.
- May use: `serde`, `thiserror`, `uuid` (external crates for serialization/errors)
- Must NOT use: async, I/O, env vars, global state

**Repo** (`crates/ports/outbound/repo/`, crate: `repo`)
- Trait definitions describing what the application needs from infrastructure (repository traits, external service traits).
- Depends only on `domain-types`.
- Must NOT use: concrete types, async runtimes, I/O

**Commands** (`crates/app/commands/`, crate: `commands`)
- Business logic that orchestrates domain types via port traits. Takes `&impl PortTrait` — never imports concrete adapters.
- Depends on `domain-types` + `repo`.
- Must NOT use: database queries, HTTP calls, file I/O, global state

**DB Adapter** (`crates/adapters/outbound/db/`, crate: `db`)
- Concrete implementations of outbound port traits (e.g. `PostgresRepo implements OrderRepository`).
- Depends on `domain-types` + `repo`.
- May use: SQLx, reqwest, file I/O (if needed)

**API** (`crates/adapters/inbound/api/`, crate: `api`)
- Axum router, handlers, main.rs — the composition root. The ONLY place where adapters meet app logic. Instantiates adapters and passes them to use cases.
- Only crate allowed to read env vars (via `#[allow]`).
- Depends on everything.

### Error Flow

Domain errors (`OrderError::NotFound`) bubble via `?`. `From<DomainError> for AppError` in server crate converts to HTTP status. Internal/External variants log real message server-side, return generic message to client.

### Per-Crate clippy.toml Enforcement

Each inner crate has a `clippy.toml` that bans operations inappropriate for its layer. clippy.toml does NOT support inheritance — each file is self-contained and includes all workspace-level bans plus layer-specific bans.

**Workspace-wide bans** (root `clippy.toml`):
- `std::env::var*` — use centralized config module
- `std::fs::*` — use centralized IO module
- `std::thread::sleep` — use `tokio::time::sleep`
- `std::process::exit` — return Result from main
- `reqwest::Client::new/builder` — inject shared client via DI
- `HashMap`/`HashSet` — use BTree variants for deterministic ordering
- `std::sync::Mutex`/`RwLock` — use parking_lot
- `std::fs::File` — no direct file handle construction

**Layer-specific bans** (domain/types/, ports/outbound/repo/, app/commands/):
- `LazyLock`, `OnceLock`, `once_cell::*` — no global state in business logic

When adding a new inner crate, copy `clippy.toml` from the nearest layer equivalent and update the ban reason strings to reflect the new crate's purpose. Also add `[lints] workspace = true` to the new crate's `Cargo.toml` to inherit workspace-level lint settings.

## How to Add a New Module

Follow this order — each step depends on the previous:

1. **Domain types** (`crates/domain/types/src/`) — entities, value objects, domain error enum. Zero dependencies.
2. **Port traits** (`crates/ports/outbound/repo/src/`) — trait definitions using domain types. Use `trait-variant` for async traits.
3. **App use cases** (`crates/app/commands/src/`) — takes `&impl PortTrait`, never imports concrete adapters.
4. **Adapters** (`crates/adapters/outbound/db/src/`) — concrete implementations of port traits (SQLx, reqwest).
5. **API handlers** (`crates/adapters/inbound/api/src/handlers/`) — Axum handlers using `State(state)` + `ValidatedJson<T>`.
6. **Wire in main.rs** — instantiate adapters, pass to use cases via `AppState`. Only place concrete adapters are created.

Follow patterns in existing crates. Copy `clippy.toml` from the nearest layer equivalent.

## Error Handling

All errors use the `AppError` enum in `adapters/inbound/api/src/error.rs`:
- `BadRequest(String)` → 400 — client input errors
- `Internal(String)` → 500 — server-side failures (logged)
- `External(String)` → 502 — upstream service failures (logged)

Implement `From<DomainError> for AppError` in the api crate. Map domain variants to `BadRequest` (client errors) or `Internal` (server errors). `Internal` and `External` variants log the real error server-side but return a generic message to the client.

## Structural Health Enforcement

The pre-commit hook enforces file-level structural limits on all `.rs` files (excluding `target/` and `tests/`):

- **Max 500 effective lines per file** (blanks + comments excluded). Split large files into modules.
- **Max 20 `use` statements per file**. High import count = too much coupling.
- **Zero `#![allow(...)]`** — crate-wide lint suppression is banned. Exception: `#![allow(unused_crate_dependencies)]` in `main.rs` (bin crates get false positives).
- **`cargo machete`** — detects unused dependencies in `Cargo.toml` (~0.07s, regex-based). If it reports false positives with proc macros, add exceptions to `.cargo-machete.toml`.

## Don'ts

1. **Don't add dependencies pointing the wrong direction.** Domain cannot import ports. Ports cannot import adapters. App cannot import adapters (except dev-deps for tests).
2. **Don't use `HashMap` or `HashSet`.** Use `BTreeMap`/`BTreeSet` for deterministic ordering. Clippy enforces this.
3. **Don't read env vars directly.** Only `main.rs` reads env vars (with `#[allow(clippy::disallowed_methods)]`). Everything else receives config via function parameters.
4. **Don't construct `reqwest::Client` per-request.** Create once at startup, inject via AppState.
5. **Don't use `std::sync::Mutex`.** Use `parking_lot::Mutex`.
6. **Don't use `unsafe`.** It's `forbid` at the workspace level.
7. **Don't use `anyhow`.** Use `thiserror` for typed error enums.
8. **Don't skip clippy.toml for new crates.** Every inner crate (domain/types, ports/outbound/repo, app/commands) needs one with global state bans.
9. **Don't use `chrono`.** Use the `time` crate.
10. **Don't add `tokio/full` feature.** List features explicitly.
11. **Don't use `#![allow(...)]` to suppress lints crate-wide.** Exception: `#![allow(unused_crate_dependencies)]` in `main.rs`. The pre-commit hook scans for this.
16. **Don't add `#[allow(...)]` without a justification comment.** Every item-level `#[allow]` must have a `//` comment on the same line explaining why. The pre-commit hook enforces this.
17. **Don't relax guardrail configs.** Changing `"deny"` to `"warn"`/`"allow"` in Cargo.toml, clippy.toml, or deny.toml is blocked. Use `# EXCEPTION: reason` if genuinely needed.
12. **Don't create files over 500 effective lines.** The pre-commit hook enforces this. Split into modules.
13. **Don't index directly.** `vec[i]` panics — use `.get(i)` and handle `None`. Clippy `indexing_slicing` enforces this.
14. **Don't slice strings directly.** `&s[0..5]` panics on non-ASCII — use `.get(0..5)` or `.chars()`. Clippy `string_slice` enforces this.
15. **Don't use bare arithmetic.** `a + b` can overflow — use `.checked_add()` / `.saturating_add()`. Clippy `arithmetic_side_effects` enforces this.
