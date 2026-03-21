# Parser Development Pipeline v4

## Philosophy

Build the parser first, test it against real-world files, fix it iteratively. Use existing library analysis as a test suite at the end, not a design input at the beginning. Lossless parsing means nothing is rejected — every byte goes somewhere.

## Steps

### Step 1: Acquire real-world files

**Input:** URL path template + domain list
**Output:** `raw-files/0001.txt ... NNNN.txt` + `raw-files/manifest.json`

Fetch the target file from the top 100K domains. No rate limiting needed — one request per domain, 10K concurrent, fire and forget. ~2 min wall time for 100K domains.

```
fetch(
  domains: Tranco top 100K,
  path: "/.well-known/security.txt",
  concurrency: 10_000,
  timeout_per_request: 5s,
)
```

Save every 200 response as a numbered file. Manifest records domain, HTTP status, content-type, file size, response time.

**Tool:** standalone binary in pipelin3r monorepo. Generic — works for any file type (security.txt, robots.txt, ads.txt, humans.txt).

### Step 2: Research the spec

**Input:** format name (e.g., "security.txt RFC 9116")
**Output:** `spec/field-list.json` + `spec/spec-reference.md`

LLM agent reads the RFC/spec, produces:
- `field-list.json`: array of `{name, required, syntax, description}` for every spec-defined field
- `spec-reference.md`: full spec analysis (what we have from step 8 of v3)

The field list is the lookup table: if a field name is in this list, it's first-class. Everything else is non-standard.

### Step 3: Design parser types

**Input:** `spec/field-list.json` + robots.txt parser types (template)
**Output:** Rust type definitions (`types.rs`)

LLM agent reads the field list and the robots.txt parser `types.rs` as a structural template. Produces the equivalent types for the new format:
- One struct per spec-defined field (with value, line number, formatting)
- `NonStandardField` for unrecognized but parseable fields
- `UnknownLine` for unparseable lines
- `Comment` for comment lines
- `LineFormatting`, `LineEnding`, `EmptyLine` for lossless reconstruction
- Top-level `ParsedSecurityTxt` (or whatever format)

Verified with script breaker: all spec fields have a corresponding type, the struct has `reconstruct()` support fields.

### Step 4: Write the parser

**Input:** `types.rs` + `spec/field-list.json` + `spec/spec-reference.md`
**Output:** `parser.rs` + `reconstruct.rs`

LLM agent writes:
- `parser.rs`: line-by-line parser that populates the types. Handles field extraction, comment detection, separator capture, line ending tracking.
- `reconstruct.rs`: rebuilds the original bytes from the parsed struct.

Verified with script breaker: `cargo test` passes, basic smoke tests work.

### Step 5: Lossless iteration against real-world files

**Input:** `raw-files/*.txt` + parser binary
**Output:** lossless reconstruction report

Script (no LLM):
```
for each file in raw-files/:
  parsed = parse(file)
  reconstructed = reconstruct(parsed)
  assert reconstructed == original bytes
```

Failures get collected. LLM fixer reads the failures and fixes the parser. Iterate until 100% pass rate on all real-world files.

This is where the parser gets battle-tested against real formatting variations: mixed line endings, BOM, trailing whitespace, inline comments, no trailing newline, etc.

### Step 6: Analyze non-standard fields

**Input:** parse results from step 5
**Output:** `non-standard-fields.json`

Script (no LLM):
```
for each parsed file:
  collect all NonStandardField entries
aggregate by field name
sort by frequency
```

Output: `{name, count, example_values}` for every non-standard field that appeared. Human (or LLM) reviews and decides which to add to the recognized non-standard list (like robots.txt's crawl-delay, host, noindex).

### Step 7: Run existing library test fixtures

**Input:** fixtures from v3 pipeline (399 files) + parser binary
**Output:** fixture comparison report

Parse each fixture with the new parser. Compare against the wrapper outputs from the 23 existing libraries (already collected in ground-truth/). Find cases where the new parser disagrees with the majority. Investigate each disagreement — is the new parser wrong, or are the libraries wrong?

This is the quality gate. The existing library analysis becomes a test suite, not a design input.

### Step 8: Package and publish

Cargo metadata, README, `cargo publish --dry-run`, release.

## What's reusable from v3

| v3 step | v4 usage |
|---------|----------|
| Steps 2-3 (find libraries, extract source) | Not needed for parser dev. Useful for wrapper generation. |
| Step 4 (clone, install, extract tests) | Step 7 test input |
| Step 5 (generate wrappers) | Step 7 comparison data |
| Step 6 (run test suites) | Step 7 comparison data |
| Step 6b (run fixtures) | Step 7 comparison data |
| Step 7 (classify) | Step 7 classification |
| Step 8 (research spec) | Step 2 input |

## New tools needed

1. **File fetcher binary** (step 1) — generic concurrent HTTP fetcher. Input: domain list + path. Output: directory of files.
2. **Lossless verifier** (step 5) — parse + reconstruct + compare for a directory of files.
3. **Field analyzer** (step 6) — aggregate non-standard fields from parse results.

All three are scripts/binaries, no LLM needed.
