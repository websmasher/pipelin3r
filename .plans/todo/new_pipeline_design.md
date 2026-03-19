# pipelin3r Pipeline Design — Handoff Document

**Date:** 2026-03-19
**Status:** Design agreed, ready for implementation

## Context

This document captures the agreed pipeline architecture for building Rust parser libraries using pipelin3r. It covers the step primitives, the doer-breaker-fixer pattern, the pipeline steps for parser development, and all implementation details discussed.

The pipelin3r library already exists with working low-level primitives (`run_agent`, `PipelineContext`, `run_pool_map`, etc.). This design adds higher-level composition patterns on top.

## What Exists in pipelin3r Today

### Low-level primitives (implemented, tested, working)
- `Executor` — wraps shedul3r SDK, handles auth, env forwarding, local/remote detection
- `AgentConfig` + `executor.run_agent(&config)` — execute a Claude Code agent via shedul3r
- `PipelineContext` — manages base_dir, creates temp dirs for remote execution with only declared inputs
- `run_pool_map(items, concurrency, total, f)` — bounded concurrent execution
- `TemplateFiller` — injection-safe prompt template variable replacement
- `validate_and_fix` — convergence loop (exists but needs redesign per below)
- `run_command` — shell command execution
- Async polling via shedul3r for remote execution through proxies
- All tested with 490+ tests including adversarial rounds

### What was tested end-to-end
- 6-step pipeline (scaffold → research → extract libraries → clone+tests → clone+source → filter tests)
- Works locally and remotely through Railway-hosted shedul3r
- 9 languages researched, 17 libraries extracted, tests and source extracted via tree-sitter

---

## Step Primitives

Every pipeline step is one of two types:

### 1. Script Step
Deterministic. No LLM. A Rust function that takes a directory path and does work.
```rust
fn step_clone_repos(base_dir: &Path) -> Result<(), PipelineError> {
    // git clone, tree-sitter extract, run parsers, compare outputs, etc.
}
```
Examples: clone repos, install packages, tree-sitter extraction, run reference parsers, compare outputs, measure coverage, cargo test, cargo fuzz.

### 2. LLM Step (AgentStep)
An agent call with explicit prompt template, input files, output files, and template variables.
```rust
AgentStep {
    prompt_template: "prompts/09_doer.md",     // path to prompt template file
    vars: vec![
        Var::String("{{PACKAGE_NAME}}", "websmasher-security-txt-parser"),
        Var::String("{{OUTPUT_PATH}}", "rulings.json"),
        Var::File("{{SPEC}}", "spec-reference.md"),  // reads file content, embeds in prompt
    ],
    inputs: vec!["disagreements.json", "spec-reference.md"],  // files needed in work_dir
    outputs: vec!["rulings.json"],                              // files the agent will produce
}
```

**Template variable resolution:**
- `Var::String(placeholder, value)` — literal string replacement
- `Var::File(placeholder, path)` — reads file from work_dir, embeds content in prompt

**Input paths:**
- Relative to pipeline base_dir
- Can be files (`"disagreements.json"`) or directories (`"research/rust/"` — includes all files recursively)
- For remote execution: only declared inputs are uploaded (temp dir with just these files)

**Output paths:**
- Relative to the step's output directory
- After execution: verified to exist, downloaded if remote, copied back to base_dir

---

## The Doer-Breaker-Fixer Pattern

Most LLM steps need self-verification. The pattern has three distinct roles, each with its own prompt and inputs:

### Doer
First pass. Gets the task inputs, produces output from scratch.
- Runs ONCE, never re-enters the loop.
- Prompt: "Here's the task. Here are the inputs. Produce X."

### Breaker
Reviews the doer's (or fixer's) output. Tries to find problems.
- Gets: the current output + whatever else it needs (spec, requirements, criteria)
- Does NOT necessarily get: doer's original inputs (depends on the step)
- Must get: the output to review (always)
- Prompt: "Here's what was produced. Find everything wrong with it."
- Output: issues text, or "No issues found"

Two flavors:
- **Script breaker**: `fn(&Path) -> Result<(), String>` — takes output file path, returns issues string or Ok. Does NOT write files. Examples: JSON validator, schema checker, `cargo check`.
- **LLM breaker**: `AgentStep` — a full agent that reads the output and writes an issues file.

Multiple breakers run in sequence. First the script breakers (fast, cheap), then the LLM breaker (slow, expensive). All issues are collected into a single issues file with headers indicating which breaker produced each section.

### Fixer
Gets everything it needs to fix the issues. Produces a corrected version of the output.
- Gets: the current output + issues + whatever else it needs (varies per step)
- Does NOT necessarily get: doer's original inputs (depends on the step)
- Prompt: "Here's what was produced. Here's what's wrong. Fix it."
- Output: fixed version of the output (NEW file, not overwriting original)

### The Loop
```
doer runs once → output
breaker checks output → issues or "ok"
if issues:
    fixer gets output + issues → fixed output
    breaker checks fixed output → issues or "ok"
    loop fixer → breaker until "ok" or max_iterations
```

The doer NEVER re-enters. The loop is ONLY between fixer and breaker.

### Directory Structure (no overwriting)
Every iteration produces new files in its own directory:
```
step-09-resolve/
  iter-0/                    # doer
    disagreements.json       # input (copied in)
    spec-reference.md        # input (copied in)
    rulings.json             # doer's output
  iter-1/                    # first breaker + fixer cycle
    rulings.json             # output being checked (copied from iter-0)
    issues.md                # breaker's findings (all breakers combined)
    rulings-fixed.json       # fixer's output
  iter-2/                    # second cycle
    rulings.json             # output being checked (copied from iter-1 fixer)
    issues.md                # breaker says "No issues found" → converged
  rulings-final.json         # symlink or copy of the last good output
```
Full chain of evidence preserved. Nothing overwritten. Every intermediate visible for debugging.

### Issues File Format
The orchestrator combines all breaker outputs into one file:
```markdown
## Format validation
Invalid JSON at line 42 col 5: expected comma

## Adversarial review
Ruling #3 contradicts RFC 9116 section 4.2. The spec says...
```
Script breakers contribute their error string. LLM breakers contribute whatever the agent wrote. Headers identify the source.

---

## High-Level API

### Hierarchy
```
run_agent (lowest level — call shedul3r, get result)
  ↑ used by
AgentStep (adds: prompt template loading, var filling, input/output declaration)
  ↑ used by
run_verified_step (adds: doer/breaker/fixer wiring, iteration dirs, loop)
  ↑ used by
pipeline runner (the actual step sequence)
```

Each level is independently usable. If `run_verified_step` doesn't fit a step's needs, drop to `AgentStep` or raw `run_agent`. Nothing breaks — higher levels are helper functions, not a framework.

### run_verified_step
```rust
let result = run_verified_step(
    &executor,
    &work_dir,
    VerifiedStep {
        name: "09-resolve",
        doer: AgentStep {
            prompt_template: "prompts/09_resolve/doer.md",
            vars: vec![
                Var::String("{{OUTPUT_PATH}}", "rulings.json"),
                Var::File("{{SPEC}}", "spec-reference.md"),
            ],
            inputs: vec!["disagreements.json", "spec-reference.md"],
            outputs: vec!["rulings.json"],
        },
        breakers: vec![
            Breaker::Script(json_validator),
            Breaker::Agent(AgentStep {
                prompt_template: "prompts/09_resolve/breaker.md",
                vars: vec![
                    Var::File("{{RULINGS}}", "rulings.json"),
                    Var::File("{{SPEC}}", "spec-reference.md"),
                ],
                inputs: vec!["rulings.json", "spec-reference.md"],
                outputs: vec!["issues.md"],
            }),
        ],
        fixer: AgentStep {
            prompt_template: "prompts/09_resolve/fixer.md",
            vars: vec![
                Var::String("{{OUTPUT_PATH}}", "rulings-fixed.json"),
                Var::File("{{CURRENT_RULINGS}}", "rulings.json"),
                Var::File("{{ISSUES}}", "issues.md"),
                Var::File("{{DISAGREEMENTS}}", "disagreements.json"),
            ],
            inputs: vec!["rulings.json", "issues.md", "disagreements.json"],
            outputs: vec!["rulings-fixed.json"],
        },
        max_iterations: 3,
    },
).await?;
```

### What run_verified_step does (all the glue)
1. Creates `{work_dir}/{name}/iter-0/` directory
2. Copies doer's declared inputs from base_dir into iter-0
3. Loads doer's prompt template, fills vars (Var::File reads from iter-0 dir)
4. Calls `run_agent` with iter-0 as work_dir
5. Verifies doer's outputs exist
6. Runs breakers in sequence:
   - Script breakers: call function with output file path, collect error strings
   - LLM breakers: copy needed files to iter dir, fill prompt, call run_agent, read issues file
7. If all breakers pass → done, copy final output to `{name}/final/`
8. If issues found → create iter-1 dir, write combined issues.md
9. Copy fixer's declared inputs from iter-0 (or wherever they are) into iter-1
10. Load fixer's prompt template, fill vars
11. Call `run_agent` for fixer
12. Go to step 6 with fixer's output
13. Repeat until converged or max_iterations
14. Copy final output to `{name}/final/`

### Dropping to lower level
If a step doesn't fit the pattern:
```rust
// Manual — no run_verified_step, just raw calls
let dir = work_dir.join("weird-step");
fs::create_dir_all(&dir)?;
fs::copy(base_dir.join("input.json"), dir.join("input.json"))?;

let prompt = TemplateFiller::new()
    .set("{{THING}}", "value")
    .fill(&fs::read_to_string("prompts/weird.md")?);

executor.run_agent(&AgentConfig {
    name: String::from("weird-step"),
    prompt,
    work_dir: Some(dir),
    expect_outputs: vec![String::from("output.json")],
    ..defaults
}).await?;
```

---

## Parser Development Pipeline Steps

Based on `/Users/tartakovsky/Projects/websmasher/websmasher/.plans/todo/new_parser_pipeline.md`

### Phase 1: Research

| Step | Type | What it does |
|------|------|-------------|
| 1. Scaffold | LLM + adversarial | Generate architecture.md (scope, API, types) |
| 2. Find libraries | LLM + adversarial (per language, batch) | Research libraries across 9 languages |
| 3. Clone + install | Script | Clone repos, install reference parsers via package managers |
| 4. Extract test fixtures | Script (tree-sitter) | Extract INPUT DATA from test suites (language-agnostic fixtures) |
| 5. Extract source | Script (tree-sitter) | Extract parser implementation source code |
| 6. Research spec | LLM + adversarial | Research RFC, produce spec-reference.md |

### Phase 2: Ground Truth

| Step | Type | What it does |
|------|------|-------------|
| 7. Run reference parsers | Script | Run ALL installed parsers on ALL fixtures, capture outputs |
| 8. Classify results | Script | Consensus / majority / split / unique / all-crash per fixture |
| 9. Resolve disagreements | LLM + adversarial (DECISION POINT) | Rule which parser is correct based on spec |
| 10. Produce test catalog | Script + LLM | Combine consensus + rulings into verified test catalog |

### Phase 3: Gap Analysis

| Step | Type | What it does |
|------|------|-------------|
| 11. Coverage analysis | Script | Map test catalog to spec sections, find gaps |
| 12. Generate edge cases | LLM + validator convergence | Generate edge cases, validate against reference parsers |

### Phase 4: Test Implementation

| Step | Type | What it does |
|------|------|-------------|
| 13. Implement tests | LLM + validator convergence | Generate Rust test files, converge until `cargo check` passes |

### Phase 5: Implementation

| Step | Type | What it does |
|------|------|-------------|
| 14. Implement parser | LLM + validator convergence | Implement parser, converge until `cargo test` passes |
| 15. Round-trip verification | Script (EARLY GATE) | `reconstruct(parse(input)) == input` on all fixtures |

### Phase 6: Cross-Validation (DECISION POINT)

| Step | Type | What it does |
|------|------|-------------|
| 16. Cross-validate | Script + LLM | Run our parser + references on real-world corpus, classify disagreements |

### Phase 7: Hardening

| Step | Type | What it does |
|------|------|-------------|
| 17. Adversarial inputs | LLM + validator convergence | Generate hostile inputs, parser must not crash |
| 18. Fuzz testing | Script | cargo-fuzz, 1M+ iterations |
| 19. Code coverage | Script | cargo-llvm-cov, flag uncovered paths |

### Key Design Decisions

1. **Ground truth from running code, not LLM translation.** Step 7 runs actual parsers on fixtures. Expected values come from parser consensus, not LLM interpretation.

2. **Disagreements resolved explicitly.** Step 9 doesn't assume any parser is correct. Where parsers disagree, the spec is consulted. Where the spec is ambiguous, the ruling is documented with reasoning.

3. **Round-trip is an early gate (step 15).** If the parser loses information, stop and fix before hardening.

4. **Cross-validation against references (step 16).** Our parser isn't "correct" because it passes its own tests — it's correct because it matches what battle-tested parsers produce on real-world data, OR because it demonstrably follows the spec better than they do.

5. **Two real decision points** where LLM judgment matters: step 9 (resolve disagreements) and step 16 (cross-validation classification). Everything else is either deterministic or recoverable with retries.

---

## Implementation Plan

### What to build in pipelin3r

1. **`AgentStep` struct** with `prompt_template`, `vars: Vec<Var>`, `inputs`, `outputs`
   - `Var::String(placeholder, value)` and `Var::File(placeholder, path)`
   - Step executor: load template from file, fill vars, call `run_agent`

2. **`Breaker` enum**
   - `Breaker::Script(fn(&Path) -> Result<(), String>)` — returns issues text or Ok
   - `Breaker::Agent(AgentStep)` — LLM that writes issues file

3. **`VerifiedStep` struct** with `name`, `doer: AgentStep`, `breakers: Vec<Breaker>`, `fixer: AgentStep`, `max_iterations: usize`

4. **`run_verified_step` function** — the orchestrator that handles directory creation, file copying, iteration loop, issues collection, convergence detection

5. **Keep all existing low-level primitives** — `run_agent`, `PipelineContext`, `run_pool_map`, etc. The new types compose on top, don't replace.

### What to build in the pipeline runner (dev-process-v2 or new tool)

1. Prompt templates for each step (doer/breaker/fixer variants)
2. Script steps for deterministic work
3. Reference parser wrapper scripts (one per language)
4. Step sequencing with fail-stop
5. Real-world corpus for cross-validation testing

### What NOT to change

- shedul3r server — working, deployed, async API functional
- shedul3r SDK — working, polling, retry all functional
- Executor, auth, remote transport — all working
- TemplateFiller — working, used by AgentStep internally

---

## Bugs and Lessons from Previous Session

### Critical bugs found and fixed
1. actix multipart rejects slashes in part names — SDK uses numeric part names, server reads Content-Disposition filename
2. ApiElapsed deserialization mismatch (float vs struct)
3. CLAUDE_CONFIG_DIR must not be forwarded to remote
4. Empty work_dir sends local path to remote — always upload bundle
5. Download must be gated on task success
6. Actix-web default timeouts too short (5s)
7. Stdout truncation needed (32KB cap)
8. Cloudflare proxy timeouts — disabled proxies, added async polling

### Lessons for pipeline design
- Don't use LLM for programmatic tasks (tree-sitter extraction is deterministic)
- Don't add filtering that doesn't exist in the reference pipeline
- Don't destroy previous run outputs — save for comparison
- Pipeline must fail-stop on errors, not continue
- shedul3r retry handles transient failures — don't double-retry in pipeline
- Log the full AgentConfig before every agent call for debuggability
- Dry-run first before spending API calls on real runs

### OAuth token management
- Tokens stored in macOS keychain: `Claude Code-credentials-{hash}`
- Hash = first 8 chars of SHA-256 of CLAUDE_CONFIG_DIR
- For trtk account: `Claude Code-credentials-11e4f183`
- For guen account: `Claude Code-credentials-dbdd1be4`
- Tokens expire — check `expiresAt` field
- Pass via `--oauth-token` flag or `CLAUDE_CODE_OAUTH_TOKEN` env var

### Railway deployment
- shedul3r deploys via `cargo binstall` on Railway
- Set `NIXPACKS_START_CMD` for install + run
- Push to `production` branch triggers GitHub Actions release build
- Cloudflare proxies MUST be disabled (DNS-only) on all claude-worker subdomains
