# Parser Pipeline v4 — Full Design Document

## What We're Building

An automated pipeline that takes a file format (security.txt, robots.txt, ads.txt, etc.) and produces a production-quality, lossless Rust parser library — with minimal human intervention.

"Lossless" means: parse the file into a typed struct, then reconstruct the struct back to bytes. The reconstructed output must be byte-identical to the original input. Nothing is rejected, nothing is lost. Every byte goes into either a recognized field, a non-standard field, an unknown line, a comment, or formatting metadata.

The pipeline handles everything from acquiring real-world sample files to publishing the crate. Each step is either a script (deterministic) or an LLM agent with adversarial review (verified).

## Why This Exists

We're building ~100 parsers for various web file formats (security.txt, robots.txt, ads.txt, humans.txt, llms.txt, DMARC, etc.). Writing each by hand is too slow. The pipeline automates the research, type design, implementation, and testing — producing parsers that match or exceed the quality of existing open-source implementations.

## The Problem We Solved Getting Here

We started with a v3 pipeline that did massive fan-out analysis: discover 23 libraries, generate wrappers for each, run 399 fixtures through all wrappers, compare outputs. This produced great research data but created a fan-in problem — synthesizing all that data into a parser was the hardest possible prompt.

v4 inverts the approach: build the parser early (informed by the spec + best existing implementation), then iterate against real files, then use the library analysis as a quality gate at the end.

## What Already Works (Built in This Session)

### Tools
- **f3tch** (`apps/f3tch/`) — concurrent file fetcher. Downloads a specific URL path from 100K+ domains using the Tranco top-1M list. Filters binary (infer crate), HTML/JSON/XML, and soft-404s. 100K domains in 3.4 min.
- **t3str** (`apps/t3str/`) — multi-language test discovery and execution. Extracts test function names via tree-sitter AST parsing, runs test suites for Python/Go/Rust/PHP/Ruby/JS/C#/Java/Elixir, parses structured output.
- **shedul3r** (`apps/shedul3r/`) — task execution server with resilience patterns. Runs commands and Claude Code agents on a Railway worker. REST + MCP transport.

### Infrastructure
- **Docker worker image** (v3b) — all 9 language runtimes (Python, Go, Rust, PHP, Ruby, Node, .NET, Java, Elixir), native -dev packages, clang/llvm, full sudo, rlwrap. Deployed on Railway.
- **v3 pipeline** (`websmasher/tools/dev-process-v3/`) — steps 1-8 working end-to-end. 23 libraries discovered, wrappers generated, test suites run, fixtures compared, spec researched.

### Data (for security.txt)
- 190 real-world security.txt files from top 100K domains
- 399 curated edge-case fixtures from existing library test suites
- 23 wrapper scripts (one per library) that parse security.txt via stdin→JSON
- Ground truth: fixture comparison data (5 consensus, 276 majority, 118 split)
- 586-line RFC 9116 spec reference

## Pipeline Steps

### Step 1: Acquire real-world files
**Tool:** f3tch (built)
**Input:** Tranco top-1M list + URL path (e.g., `/.well-known/security.txt`)
**Output:** `raw-files/000001.txt ... NNNNNN.txt`
**How:** 10K concurrent requests, infer+soft-404 filtering
**Status:** Done

### Step 2: Research the spec
**Tool:** LLM agent via shedul3r (reuses v3 step 8)
**Input:** RFC/spec URL or document
**Output:** `spec/field-list.json` (field names, required/optional, syntax) + `spec/grammar.md` (ABNF, line structure, special blocks like PGP) + `spec/spec-reference.md` (full analysis)
**Status:** Partially done (spec-reference.md exists from v3)

### Step 3: Find and rank existing libraries
**Tool:** v3 pipeline steps 2-4 + t3str
**Input:** format name + 9 target languages
**Output:** ranked library list with quality signals (test count, RFC mentions, completeness)
**Goal:** identify the **best reference implementation** — the one whose grammar to learn from
**Status:** Libraries discovered, tests extracted (from v3)

### Step 4: Analyze the reference parser
**Tool:** LLM agent
**Input:** best library's parser source code
**Output:** `reference/grammar-analysis.md` (actual grammar vs spec, edge cases, non-standard fields, malformed input handling)
**Why:** The spec grammar is idealized. Real parsers handle things the spec doesn't mention. For robots.txt we literally stole a NOM grammar from an existing lib.
**Status:** Not built

### Step 5: Design parser types
**Tool:** LLM agent + script breaker
**Input:** spec field list + reference grammar analysis + robots.txt parser types (template)
**Output:** `src/types.rs` in the target crate
**Pattern:** follows the robots.txt parser structure exactly (see below)
**Status:** Not built

### Step 6: Write the parser
**Tool:** LLM agent with doer→breaker→fixer loop
**Input:** types.rs + spec grammar + reference grammar + sample real-world files
**Output:** `src/parser.rs` + `src/reconstruct.rs`
**Status:** Not built

### Step 7: Lossless iteration
**Tool:** script (needs building — "lossless verifier")
**Input:** all real-world files + parser binary
**How:** parse → reconstruct → byte-compare for every file. LLM fixer for failures.
**Status:** Not built

### Step 8: Analyze non-standard fields
**Tool:** script (needs building — "field analyzer")
**Input:** parse results from step 7
**Output:** frequency-ranked list of non-standard fields found in real files
**Status:** Not built

### Step 9: Library fixture quality gate
**Tool:** v3 pipeline (steps 4-7)
**Input:** existing library test fixtures + new parser
**How:** parse fixtures with new parser, compare against library behavior
**Status:** Infrastructure built, integration not built

### Step 10: Package and publish
**Tool:** script + cargo publish
**Status:** Not built

## Target Crate Structure

Every parser follows the robots.txt pattern in `websmasher-parsers/crates/`:

```
websmasher-parsers/crates/websmasher-{format}-parser/
├── Cargo.toml
├── src/
│   ├── lib.rs            # pub parse() + pub reconstruct() + re-exports
│   ├── types.rs          # ParsedFoo, field structs, LineFormatting, etc.
│   ├── parser.rs         # Line-by-line parsing, field extraction
│   ├── reconstruct.rs    # Byte-perfect reconstruction from parsed struct
│   └── grouper.rs        # (optional) semantic grouping/post-processing
└── tests/
    ├── field_parsing.rs  # Integration tests per field type
    ├── edge_cases.rs     # Malformed input, BOM, mixed line endings
    ├── signature.rs      # (if applicable) PGP/signed content
    └── comments.rs       # Comment handling
```

Public API:
```rust
pub fn parse(input: &[u8], lossless: bool) -> ParsedSecurityTxt;
pub fn reconstruct(parsed: &ParsedSecurityTxt) -> String;
```

## Files a New Agent Must Read

### To understand the overall system:
1. **This file** — `pipelin3r/.plans/2026-03-22-000000-parser-pipeline-v4-full.md`
2. **pipelin3r CLAUDE.md** — `/Users/tartakovsky/Projects/websmasher/pipelin3r/CLAUDE.md` (repo structure, workspaces, lint rules, don'ts)
3. **Global CLAUDE.md** — `/Users/tartakovsky/.claude-flar49/CLAUDE.md` (orchestration rules, agent patterns, quality standards)

### To understand the reference parser (robots.txt):
4. **robots.txt types** — `websmasher-parsers/crates/websmasher-robots-txt-parser/src/types.rs` (the template for all parser types)
5. **robots.txt parser** — `websmasher-parsers/crates/websmasher-robots-txt-parser/src/parser.rs` (parsing logic)
6. **robots.txt reconstruct** — `websmasher-parsers/crates/websmasher-robots-txt-parser/src/reconstruct.rs` (lossless reconstruction)
7. **robots.txt lib.rs** — `websmasher-parsers/crates/websmasher-robots-txt-parser/src/lib.rs` (public API surface)

### To understand the security.txt target (current parser):
8. **security.txt skeleton** — `websmasher-parsers/crates/websmasher-security-txt-parser/` (existing skeleton with 400 ignored tests)
9. **security.txt spec reference** — `websmasher/packages/websmasher-security-txt-parser-v3/research/spec/spec-reference.md` (586-line RFC 9116 analysis)
10. **security.txt test design** — `websmasher-parsers/crates/websmasher-security-txt-parser/research/test-design-final.json` (400 test cases with inputs and expected behavior)

### To understand the v3 pipeline (existing infrastructure):
11. **Pipeline config** — `websmasher/tools/dev-process-v3/src/config.rs` (install scripts, language mappings, dep recovery)
12. **Pipeline steps** — `websmasher/tools/dev-process-v3/src/steps/` (s01-s08 + s06b)
13. **Pipeline main** — `websmasher/tools/dev-process-v3/src/main.rs` (CLI, step dispatch)

### To understand the tools:
14. **f3tch** — `pipelin3r/apps/f3tch/src/main.rs` (file fetcher)
15. **t3str extract** — `pipelin3r/apps/t3str/crates/adapters/outbound/extract/src/` (test discovery per language)
16. **t3str run** — `pipelin3r/apps/t3str/crates/adapters/outbound/run/src/` (test execution per language)
17. **shedul3r MCP** — `pipelin3r/.mcp.json` (MCP server config, REST endpoint)

### To understand gotchas:
18. **OpenViking gotchas** — `viking://resources/pipelin3r/gotchas/` (OAuth token expiry, HTTP connection drops, fixture array vs discovery, soft-404 detection)
19. **Local memory** — `~/.claude-flar49/projects/-Users-tartakovsky-Projects-websmasher-pipelin3r/memory/MEMORY.md`
20. **Recent worklogs** — `pipelin3r/.worklogs/` (session decisions and context)

## Open Questions

1. **How to select the "best" library automatically?** Currently manual. Could rank by test count × test pass rate × RFC mentions in source.
2. **Should the lossless verifier be a standalone binary or a test harness in the parser crate?** Probably both — a `cargo test` integration test that runs all fixtures, plus a standalone tool for batch verification.
3. **How to handle formats without a formal spec?** Some formats (humans.txt, llms.txt) are conventions, not RFCs. Step 2 would need to scrape documentation/blog posts instead of reading an RFC.
4. **Should the pipeline generate tests alongside the parser?** The 400 tests in test-design-final.json are for security.txt specifically. For new formats, the pipeline should generate test cases from the spec + real files.
