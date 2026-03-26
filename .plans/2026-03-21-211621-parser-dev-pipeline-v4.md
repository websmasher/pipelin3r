# Parser Development Pipeline v4

## Philosophy

Lossless parsing: every byte goes somewhere — recognized fields, non-standard fields, unknown lines, comments, formatting. Nothing is rejected, nothing is lost. Reconstruction from parsed struct = original bytes.

The parser needs three knowledge sources: the spec (ideal grammar), the best existing library (real grammar), and real-world files (validation). All three feed into parser writing.

## Steps

### Step 1: Acquire real-world files

**Tool:** `f3tch` (built, tested, committed)
**Input:** Tranco top-1M domain list + URL path template
**Output:** `raw-files/000001.txt ... NNNNNN.txt` + `manifest.json` + `index.json`

Fetches the target file from top 100K domains. Filters binary (infer crate), HTML/JSON/XML, and soft-404s (probes random path). 100K domains in ~3 min.

**Status:** Done. 190 security.txt files from top 100K.

### Step 2: Research the spec

**Input:** format name + RFC/spec URL
**Output:** `spec/field-list.json` + `spec/grammar.md` + `spec/spec-reference.md`

LLM agent reads the RFC, produces:
- `field-list.json`: `{name, required, repeatable, syntax, description}` per field
- `grammar.md`: ABNF or grammar rules, line structure, special syntax (PGP blocks, comments, etc.)
- `spec-reference.md`: full analysis (reuse v3 step 8)

### Step 3: Find and rank existing libraries

**Input:** format name + language list
**Output:** `libraries/{lang}/libraries.json` + ranking of best implementation

Reuses v3 steps 2-3. Discovers libraries across all languages, extracts metadata. NEW: ranks libraries by quality signals:
- Test count (from t3str extract)
- RFC compliance mentions in source
- Star count / maintenance activity
- Parser completeness (handles PGP, comments, extensions)

Identifies the **best reference implementation** — the one to learn the real grammar from.

### Step 4: Analyze the reference parser

**Input:** best library's source code
**Output:** `reference/grammar-analysis.md` + `reference/edge-cases.md`

LLM agent reads the best library's parser source code. Extracts:
- What grammar it actually implements (vs the spec)
- Edge cases it handles that the spec doesn't cover
- Non-standard fields it recognizes
- How it handles malformed input

This is where the NOM grammar or equivalent parsing logic gets understood.

### Step 5: Design parser types

**Input:** `spec/field-list.json` + `reference/grammar-analysis.md` + robots.txt parser types (template)
**Output:** `src/types.rs`

LLM agent produces Rust type definitions following the robots.txt parser pattern:
- Typed structs for spec-defined fields (with line number, formatting)
- `NonStandardField` for recognized-but-not-spec fields
- `UnknownLine` for unparseable lines
- `Comment` for comment lines
- `LineFormatting`, `LineEnding`, `EmptyLine` for lossless reconstruction

Verified with script breaker: all spec fields have types, reconstruct support fields present.

### Step 6: Write the parser

**Input:** `src/types.rs` + spec grammar + reference grammar analysis + real-world files (sample)
**Output:** `src/parser.rs` + `src/reconstruct.rs`

LLM agent writes the parser, informed by both the spec and the reference implementation's grammar decisions. Doer→breaker→fixer loop.

### Step 7: Lossless iteration

**Input:** `raw-files/*.txt` + parser binary
**Output:** reconstruction report

Script (no LLM): parse every real-world file, reconstruct, compare bytes. Collect failures. LLM fixer reads failures and fixes the parser. Iterate until 100% pass rate.

### Step 8: Analyze non-standard fields

**Input:** parse results from step 7
**Output:** `non-standard-fields.json`

Script: aggregate `NonStandardField` entries by name, count frequency, list example values. Update the parser's recognized non-standard field list.

### Step 9: Library fixture quality gate

**Input:** existing library test fixtures (from v3 pipeline) + parser binary
**Output:** comparison report

Clone libraries, run t3str extract to get test fixtures, parse with new parser, compare against library behavior. Find disagreements, investigate each.

### Step 10: Package and publish

Cargo metadata, README, docs, `cargo publish --dry-run`, release.

## Tool inventory

| Tool | Step | Status |
|------|------|--------|
| f3tch | 1 | Built |
| v3 pipeline steps 2-3 | 3 | Built |
| t3str | 3, 9 | Built |
| LLM agents via shedul3r | 2, 4, 5, 6 | Built |
| Lossless verifier | 7 | Needs building |
| Field analyzer | 8 | Needs building |
