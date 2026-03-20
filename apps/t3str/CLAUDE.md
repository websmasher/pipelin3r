# t3str CLI — Agent Instructions

> **This is an agent-managed codebase.** The user does not read or write code directly. Your role is to own the code end-to-end. Never say "you can edit X" — just do it. Never estimate workload — just execute.

## What This Service Is

Rust CLI binary for multi-language test discovery and execution. Two subcommands:

- **`t3str extract`** — AST-based test discovery using tree-sitter. Parses source files, identifies test functions/methods, and optionally filters by topic. Outputs structured JSON describing discovered tests.
- **`t3str run`** — Test execution and structured output parsing. Invokes language-native test runners (pytest, go test, dotnet test, phpunit, etc.), captures output, and parses results into a unified JSON format.

### Where It Runs

t3str runs on the shedul3r worker machine alongside language runtimes (Python, Go, .NET, PHP, Java, etc.). It is called by the websmasher pipeline as a compiled binary — replacing the previous approach of generating Python scripts for test execution. The pipeline invokes t3str and reads JSON from stdout.

### Design Philosophy

- **Tree-sitter for discovery, not regex.** Test discovery uses AST parsing via tree-sitter grammars. This handles edge cases (nested classes, decorators, attributes, annotations) that regex cannot.
- **Native test runners for execution.** t3str does not re-implement test execution. It shells out to pytest, `go test`, `dotnet test`, phpunit, etc. and parses their structured output (JUnit XML, JSON, TAP).
- **Structured output always.** Both subcommands emit JSON to stdout. The pipeline reads this directly. Human-readable output is available via `--format human` but JSON is the default.

## Hexagonal Architecture — Required

Hexagonal architecture isolates business logic from infrastructure. Each layer is independently modifiable — agent mistakes can't cascade across boundaries. Every module follows this pattern.

### Crate Hierarchy

```
crates/
├── domain/
│   └── types/              # Crate: t3str-domain-types
│                           # Language, TestFile, TestResult, TestSuite, T3strError
│                           # ZERO internal dependencies. Pure data + logic.
│
├── ports/
│   └── outbound/
│       └── discovery/      # Crate: t3str-discovery-port
│                           # trait TestDiscoverer, trait TestExecutor
│                           # Depends only on t3str-domain-types
│
├── app/
│   └── commands/           # Crate: t3str-commands
│                           # ExtractCommand, RunCommand use cases
│                           # Depends on t3str-domain-types + t3str-discovery-port
│                           # Receives adapters via trait bounds — never imports them
│
└── adapters/
    ├── inbound/
    │   └── cli/            # Crate: t3str-cli (bin crate)
    │                       # clap CLI parser + composition root (main.rs)
    │                       # The ONLY crate that wires adapters to app layer
    │                       # Depends on everything
    │
    └── outbound/
        ├── extract/        # Crate: t3str-extract
        │                   # tree-sitter per-language test discovery
        │                   # Implements TestDiscoverer for each Language
        │                   # Depends on t3str-domain-types + t3str-discovery-port
        │
        └── run/            # Crate: t3str-run
                            # Process execution per-language + output parsers
                            # Implements TestExecutor for each Language
                            # Depends on t3str-domain-types + t3str-discovery-port
                            # Contains parsers/ submodule for JUnit XML, JSON, TAP
```

### Dependency Rules

| Crate | May depend on | Must NOT depend on |
|-------|--------------|-------------------|
| t3str-domain-types | (nothing internal) | t3str-discovery-port, t3str-commands, t3str-extract, t3str-run, t3str-cli |
| t3str-discovery-port | t3str-domain-types | t3str-commands, t3str-extract, t3str-run, t3str-cli |
| t3str-commands | t3str-domain-types, t3str-discovery-port | t3str-extract, t3str-run, t3str-cli |
| t3str-extract | t3str-domain-types, t3str-discovery-port | t3str-commands, t3str-run, t3str-cli |
| t3str-run | t3str-domain-types, t3str-discovery-port | t3str-commands, t3str-extract, t3str-cli |
| t3str-cli | everything | — (composition root) |

These rules are enforced by Cargo's dependency graph. If a crate's `Cargo.toml` doesn't list a dependency, it can't import it.

### Layer Responsibilities

**Domain Types** (`crates/domain/types/`, crate: `t3str-domain-types`)
- `Language` enum — all supported languages (Python, Go, CSharp, PHP, Java, JavaScript, TypeScript, Ruby, Rust, etc.)
- `TestFile` — represents a discovered test file with its path, language, and list of test items
- `TestItem` — a single test function/method with name, line number, topic tags
- `TestSuite` — collection of test results from a single execution
- `TestResult` — outcome of a single test (passed, failed, skipped, errored) with duration, output, failure message
- `T3strError` — domain error enum (see Error Handling below)
- May use: `serde`, `thiserror` (external crates for serialization/errors)
- Must NOT use: async, I/O, env vars, global state, filesystem, process spawning

**Discovery Port** (`crates/ports/outbound/discovery/`, crate: `t3str-discovery-port`)
- `trait TestDiscoverer` — given a file path and language, returns discovered `TestItem`s
- `trait TestExecutor` — given test files and execution config, returns `TestSuite`
- Depends only on `t3str-domain-types`
- Must NOT use: concrete types, I/O, filesystem, process spawning

**Commands** (`crates/app/commands/`, crate: `t3str-commands`)
- `ExtractCommand` — orchestrates test discovery: resolves language, walks file tree, calls `TestDiscoverer`, applies topic filters, returns structured results
- `RunCommand` — orchestrates test execution: resolves language and runner, calls `TestExecutor`, collects results, returns structured output
- Takes `&impl TestDiscoverer` / `&impl TestExecutor` — never imports concrete adapters
- Depends on `t3str-domain-types` + `t3str-discovery-port`
- Must NOT use: tree-sitter, process spawning, file I/O, global state

**Extract Adapter** (`crates/adapters/outbound/extract/`, crate: `t3str-extract`)
- Concrete `TestDiscoverer` implementations using tree-sitter grammars
- One module per language (e.g., `python.rs`, `go.rs`, `csharp.rs`, `php.rs`, `java.rs`, `javascript.rs`, `ruby.rs`, `rust.rs`)
- Each module loads the appropriate tree-sitter grammar and queries for test patterns (decorators, attributes, annotations, function naming conventions)
- Depends on `t3str-domain-types` + `t3str-discovery-port`
- May use: `tree-sitter`, `tree-sitter-*` language grammars, filesystem read (to read source files for parsing)

**Run Adapter** (`crates/adapters/outbound/run/`, crate: `t3str-run`)
- Concrete `TestExecutor` implementations per language
- Shells out to native test runners (pytest, `go test -json`, `dotnet test --logger trx`, phpunit, etc.)
- Contains `parsers/` submodule with structured output parsers (JUnit XML, Go JSON, .NET TRX, TAP, etc.)
- Depends on `t3str-domain-types` + `t3str-discovery-port`
- May use: `std::process::Command`, `quick-xml` (for JUnit/TRX parsing), filesystem access

**CLI** (`crates/adapters/inbound/cli/`, crate: `t3str-cli`)
- clap CLI definition with two subcommands: `extract` and `run`
- Composition root — the ONLY place where adapters are instantiated and wired to commands
- Handles `--format` flag (json/human), `--filter` for topic filtering, `--language` override
- Formats output and writes to stdout
- Depends on everything
- Only crate allowed to read env vars (with `#[allow]`)

## How to Build

```bash
# Debug build
cd apps/t3str && cargo build

# Release build
cd apps/t3str && cargo build --release
```

## How to Test

```bash
# All crate tests
cd apps/t3str && cargo test --workspace
```

## How to Lint

```bash
cd apps/t3str && cargo fmt --all -- --check
cd apps/t3str && cargo clippy --workspace --all-targets -- -D warnings
cd apps/t3str && cargo deny check
```

## How to Add a New Language

Follow this order — each step depends on the previous:

1. **Domain types** — Add variant to `Language` enum in `crates/domain/types/src/`. Include file extensions, test file patterns, and runner metadata. Update `serde` serialization if needed.
2. **Extract adapter** — Add module to `crates/adapters/outbound/extract/src/` implementing `TestDiscoverer` for the new language. Write tree-sitter queries that match the language's test declaration patterns (decorators, annotations, naming conventions, attributes).
3. **Run adapter** — Add module to `crates/adapters/outbound/run/src/` implementing `TestExecutor` for the new language. Define the runner command, arguments, and add a parser in `parsers/` for the runner's output format.
4. **Register** — Wire into both adapter `lib.rs` match statements so the language variant dispatches to the correct discoverer and executor.
5. **CLI picks it up automatically** — The `Language` enum drives clap's `--language` flag values. No CLI changes needed.
6. **Tests** — Add unit tests in the extract and run adapter modules. Add fixture files for the new language's test patterns.

## Per-Crate clippy.toml Enforcement

Each inner crate has a `clippy.toml` that bans operations inappropriate for its layer. clippy.toml does NOT support inheritance — each file is self-contained and includes all workspace-level bans plus layer-specific bans.

**Workspace-wide bans** (root `clippy.toml`):
- `HashMap`/`HashSet` — use BTree variants for deterministic ordering
- `std::env::var*` — use centralized config / CLI args
- `std::thread::sleep` — use `tokio::time::sleep` if async, or avoid entirely
- `std::process::exit` — return Result from main
- `std::sync::Mutex`/`RwLock` — use parking_lot
- `std::fs::File` — no direct file handle construction outside adapters

**Domain + Ports + Commands bans** (in addition to workspace-wide):
- `std::fs::*` — no filesystem access in business logic
- `std::process::Command` — no process spawning in business logic
- `LazyLock`, `OnceLock`, `once_cell::*` — no global state in business logic
- `tree_sitter::*` — tree-sitter is an adapter concern, not business logic

**Extract adapter allows:**
- `std::fs::read_to_string` — must read source files for AST parsing
- `tree_sitter::*` — its primary dependency

**Run adapter allows:**
- `std::process::Command` — must invoke test runners
- `std::fs::*` — must read test output files (JUnit XML, TRX, etc.)

When adding a new inner crate, copy `clippy.toml` from the nearest layer equivalent and update the ban reason strings to reflect the new crate's purpose. Also add `[lints] workspace = true` to the new crate's `Cargo.toml` to inherit workspace-level lint settings.

## Error Handling

`T3strError` enum in `crates/domain/types/` with variants:

- `LanguageDetectionFailed` — could not determine language from file extension or `--language` flag
- `ParseError` — tree-sitter failed to parse a source file (includes file path and language)
- `NoTestsFound` — extraction found zero test items (may or may not be an error depending on context)
- `RunnerNotFound` — the test runner binary (pytest, go, dotnet, etc.) is not installed or not in PATH
- `RunnerFailed` — the test runner process exited with a non-zero code (includes stderr)
- `OutputParseError` — failed to parse runner output (JUnit XML malformed, unexpected JSON structure, etc.)
- `IoError` — filesystem or process I/O failure (wraps `std::io::Error`)
- `FilterError` — invalid topic filter expression

Each adapter wraps language-specific errors into `T3strError` variants using `From` implementations. The CLI crate formats errors as JSON (default) or human-readable depending on the `--format` flag. Errors go to stderr; structured output goes to stdout.

## Output Format

Both subcommands output JSON to stdout by default. The pipeline reads this directly.

**`t3str extract` output:**
```json
{
  "files": [
    {
      "path": "tests/test_auth.py",
      "language": "python",
      "items": [
        {
          "name": "test_login_success",
          "line": 15,
          "topics": ["auth", "login"]
        }
      ]
    }
  ],
  "summary": {
    "total_files": 1,
    "total_tests": 1,
    "languages": ["python"]
  }
}
```

**`t3str run` output:**
```json
{
  "suite": {
    "tests": [
      {
        "name": "test_login_success",
        "status": "passed",
        "duration_ms": 42,
        "output": "",
        "failure_message": null
      }
    ],
    "summary": {
      "total": 1,
      "passed": 1,
      "failed": 0,
      "skipped": 0,
      "errored": 0,
      "duration_ms": 150
    }
  }
}
```

Use `--format human` for human-readable table output (intended for local development, not pipeline consumption).

## Structural Health Enforcement

The pre-commit hook enforces file-level structural limits on all `.rs` files (excluding `target/` and `tests/`):

- **Max 500 effective lines per file** (blanks + comments excluded). Split large files into modules.
- **Max 20 `use` statements per file**. High import count = too much coupling.
- **Zero `#![allow(...)]`** — crate-wide lint suppression is banned. Exception: `#![allow(unused_crate_dependencies)]` in `main.rs` (bin crates get false positives).
- **`cargo machete`** — detects unused dependencies in `Cargo.toml`. If it reports false positives with proc macros, add exceptions to `.cargo-machete.toml`.

## Don'ts

1. **Don't add dependencies pointing the wrong direction.** Domain cannot import ports. Ports cannot import adapters. App cannot import adapters (except dev-deps for tests).
2. **Don't use `HashMap` or `HashSet`.** Use `BTreeMap`/`BTreeSet` for deterministic ordering. Clippy enforces this.
3. **Don't read env vars directly.** Only `main.rs` reads env vars (with `#[allow(clippy::disallowed_methods)]`). Everything else receives config via function parameters or CLI args.
4. **Don't use `unsafe`.** It's `forbid` at the workspace level.
5. **Don't use `anyhow`.** Use `thiserror` for typed error enums.
6. **Don't skip clippy.toml for new crates.** Every inner crate needs one with layer-appropriate bans.
7. **Don't use `chrono`.** Use the `time` crate.
8. **Don't add `tokio/full` feature.** List features explicitly.
9. **Don't use `#![allow(...)]` to suppress lints crate-wide.** Exception: `#![allow(unused_crate_dependencies)]` in `main.rs`. The pre-commit hook scans for this.
10. **Don't add `#[allow(...)]` without a justification comment.** Every item-level `#[allow]` must have a `//` comment on the same line explaining why. The pre-commit hook enforces this.
11. **Don't relax guardrail configs.** Changing `"deny"` to `"warn"`/`"allow"` in Cargo.toml, clippy.toml, or deny.toml is blocked. Use `# EXCEPTION: reason` if genuinely needed.
12. **Don't create files over 500 effective lines.** The pre-commit hook enforces this. Split into modules.
13. **Don't index directly.** `vec[i]` panics — use `.get(i)` and handle `None`. Clippy `indexing_slicing` enforces this.
14. **Don't slice strings directly.** `&s[0..5]` panics on non-ASCII — use `.get(0..5)` or `.chars()`. Clippy `string_slice` enforces this.
15. **Don't use bare arithmetic.** `a + b` can overflow — use `.checked_add()` / `.saturating_add()`. Clippy `arithmetic_side_effects` enforces this.
16. **Don't use `unwrap()` or `expect()`.** Use `?` with proper error types, or `.ok_or_else()` / `.map_err()`. Clippy enforces this.
17. **Don't use regex for parsing test output.** Use proper XML parsers (`quick-xml`) for JUnit/TRX, `serde_json` for JSON output, and dedicated parsers for TAP format.
18. **Don't use grep/regex for test discovery.** Use tree-sitter AST analysis. Regex cannot reliably handle nested structures, decorators, attributes, or multiline annotations.
19. **Don't embed scripts in other languages.** Everything is compiled Rust. No Python/Bash/etc. scripts generated or embedded at runtime.
20. **Don't use the `regex` crate directly.** It is banned in deny.toml (allowed only as a transitive dependency of tree-sitter). If you need pattern matching, use tree-sitter queries or string methods.
21. **Don't use `std::sync::Mutex`.** Use `parking_lot::Mutex`.
22. **Don't construct process commands outside the run adapter.** Only `t3str-run` is allowed to spawn child processes. The extract adapter reads files but never executes them.
