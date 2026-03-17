# Structured Output Composition Block

**Date:** 2026-03-17 18:08
**Type:** Design document (agent output)

---

# Structured Output Composition Block for pipelin3r

## Design Plan

### Goal

Add a `StructuredOutputBuilder<T>` that wraps `AgentBuilder`, executing an LLM agent and then reading, parsing, and validating a typed output file from the work directory. On parse/validation failure, it retries the agent with the error message appended to the prompt, implementing the Instructor/PydanticAI pattern in Rust.

### Key Design Decisions

**1. Wrapper around AgentBuilder, not a new builder type**

`StructuredOutputBuilder<T>` is a new builder that internally owns/configures an `AgentBuilder`. It is NOT a replacement. Rationale:
- `AgentBuilder` already handles auth, model, timeout, tools, work_dir, expected_outputs, dry-run, local/remote transport. Duplicating all that is wasteful and creates a maintenance burden.
- The structured output concern is orthogonal: it is a post-processing + retry loop on top of agent execution.
- Users access it via `executor.structured_output::<T>("step-name")` which returns the new builder, or via a `.parse_output::<T>()` conversion method on `AgentBuilder`.

I recommend the conversion approach: `executor.agent("name").prompt(...).work_dir(...).parse_output::<T>("output.json")` returns a `StructuredOutputBuilder<T>`. This keeps the entry point familiar and makes it clear that structured output is an agent with parsing bolted on.

**2. Retry re-runs the whole agent**

The retry must re-execute the full agent (new shedul3r task submission) because:
- The agent writes files to the work_dir. A "re-prompt" without re-execution doesn't make sense in this architecture -- the LLM runs inside shedul3r, not in a conversational loop.
- Each retry appends the parse error to the prompt, so the LLM gets the feedback.
- The work_dir already contains the previous (bad) output, which the agent can see and fix.

**3. Validation: closure, not trait**

Use a closure `Fn(&T) -> Result<(), String>` for custom validation. Rationale:
- A trait (`Validate`) would require users to impl a trait on every output struct, which is heavyweight for one-off validations.
- A closure composes naturally: `|v| if v.items.is_empty() { Err("items must not be empty") } else { Ok(()) }`.
- The closure is optional -- if not provided, only deserialization is checked.

**4. Error feedback injection**

On retry, the original prompt is augmented with a section like:

```
## Previous attempt failed

Your previous output could not be parsed. The error was:
{error_message}

Please fix the output and write a corrected version to {output_file}.
```

This is appended to the original prompt string. The user can customize the retry prompt format via an optional closure, but the default covers 90% of cases.

**5. Return type**

```rust
pub struct StructuredOutput<T> {
    pub data: T,
    pub agent_result: AgentResult,
    pub attempts: usize,
}
```

This gives the caller the parsed data, the raw agent result (for logging/debugging), and how many attempts it took.

### Concrete API Sketch

```rust
use pipelin3r::{Executor, Model, Tool, PipelineError};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TestDesign {
    test_files: Vec<TestFile>,
    coverage_summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TestFile {
    filename: String,
    tests: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    test_name: String,
    description: String,
}

// Usage:
let result: StructuredOutput<TestDesign> = executor
    .agent("design-tests")
    .model(Model::Sonnet)
    .tools(&[Tool::Read, Tool::Write])
    .prompt(&filled_prompt)
    .work_dir(&work_dir)
    .expect_outputs(&["test-design.json"])
    .parse_output::<TestDesign>("test-design.json")  // converts to StructuredOutputBuilder
    .max_retries(2)                                    // default: 1
    .validate(|design| {
        if design.test_files.is_empty() {
            Err(String::from("test_files must not be empty"))
        } else {
            Ok(())
        }
    })
    .execute()
    .await?;

println!("Parsed {} test files in {} attempts",
    result.data.test_files.len(),
    result.attempts);
```

### Format Support

Support JSON (default), YAML, and TOML via an enum:

```rust
pub enum OutputFormat {
    Json,
    Yaml,
    Toml,
}
```

The builder defaults to `Json` (inferred from file extension, or set explicitly). This requires adding `serde_yaml` and keeping the existing `toml` dependency. Actually -- to keep the dependency tree lean for a published crate, make YAML and TOML support feature-gated:

```toml
[features]
default = []
yaml = ["serde_yaml"]
```

`toml` is already a dependency. JSON is always available via `serde_json`. YAML requires the optional `serde_yaml` dep.

The format can be auto-detected from the file extension (`.json`, `.yaml`/`.yml`, `.toml`) or set explicitly.

### Module Structure

New module: `packages/pipelin3r/src/structured/mod.rs` (the main builder + types, under 500 lines).

Files:
- `packages/pipelin3r/src/structured/mod.rs` -- `StructuredOutputBuilder<T>`, `StructuredOutput<T>`, `OutputFormat`, the retry loop
- `packages/pipelin3r/src/structured/tests.rs` -- unit tests

### Error Handling

Add a new error variant to `PipelineError`:

```rust
/// Structured output parsing or validation failed after all retries.
#[error("structured output failed after {attempts} attempt(s): {message}")]
StructuredOutputFailed {
    /// Number of attempts made.
    attempts: usize,
    /// Last error message.
    message: String,
},
```

### Implementation Steps

**Step 1: Add `serde` dependency for `DeserializeOwned`**

Add `serde = { version = "1", features = ["derive"] }` to `Cargo.toml`. Currently only `serde_json` and `toml` are deps -- `serde` itself is needed for the `DeserializeOwned` bound.

**Step 2: Add `serde_yaml` as optional dependency**

```toml
serde_yaml = { version = "0.9", optional = true }

[features]
default = []
yaml = ["dep:serde_yaml"]
```

**Step 3: Create `structured/mod.rs`**

Core types:
- `OutputFormat` enum with `Json`, `Yaml` (feature-gated), `Toml`
- `StructuredOutput<T>` result struct
- `StructuredOutputBuilder<'a, T>` builder struct

The builder holds:
- `agent_builder: AgentBuilder<'a>` -- the underlying agent config
- `output_file: String` -- relative path in work_dir (default: `"output.json"`)
- `format: OutputFormat` -- auto-detected or explicit
- `max_retries: usize` -- default 1 (total attempts = 1 + retries)
- `validator: Option<Box<dyn Fn(&T) -> Result<(), String> + Send + Sync>>` -- custom validation
- `retry_prompt_fn: Option<Box<dyn Fn(&str, &str, &str) -> String + Send + Sync>>` -- custom (original_prompt, error, file) -> retry_prompt

The `execute()` method:
1. Run the agent via the inner `AgentBuilder`
2. Read `work_dir.join(output_file)` 
3. Parse according to `format`
4. If validator is set, run it
5. On success, return `StructuredOutput<T>`
6. On failure, if retries remain:
   a. Build retry prompt (append error context to original prompt)
   b. Rebuild `AgentBuilder` with same config but new prompt
   c. Go to step 1
7. On failure with no retries left, return `PipelineError::StructuredOutputFailed`

**Key subtlety**: `AgentBuilder::execute()` consumes `self`. For retries, we need to either:
- Clone the builder config (store the pieces, rebuild each time)
- Store the config separately and build `AgentBuilder` on each attempt

Since `AgentBuilder` borrows `&'a Executor`, we cannot clone it. Instead, `StructuredOutputBuilder` stores all the config fields (executor ref, name, model, timeout, tools, prompt, work_dir, expected_outputs, auth) and builds a fresh `AgentBuilder` for each attempt. This is actually clean -- the structured builder becomes a "replayable" agent config.

**Step 4: Add `.parse_output()` to `AgentBuilder`**

Add a method on `AgentBuilder` that converts it into a `StructuredOutputBuilder`:

```rust
pub fn parse_output<T: DeserializeOwned>(self, file: &str) -> StructuredOutputBuilder<'a, T> {
    StructuredOutputBuilder::from_agent_builder(self, file)
}
```

This extracts all fields from `AgentBuilder` into the structured builder's own storage.

**Step 5: Add `PipelineError::StructuredOutputFailed` variant**

**Step 6: Add public exports in `lib.rs`**

```rust
pub mod structured;
pub use structured::{StructuredOutput, StructuredOutputBuilder, OutputFormat};
```

**Step 7: Add `Executor::structured_output()` convenience method** (optional, for users who prefer not to go through `AgentBuilder`)

**Step 8: Write tests**

- Test format detection from extension
- Test JSON parsing success
- Test TOML parsing success
- Test validation failure triggers retry prompt construction
- Test max retries exhaustion returns error
- Integration test with dry-run mode (verify retry prompt contains error context)

### Risks and Edge Cases

1. **AgentBuilder is not Clone** -- solved by storing config fields separately in `StructuredOutputBuilder` and rebuilding `AgentBuilder` per attempt.

2. **work_dir is required** for structured output -- the output file must be read from somewhere. `execute()` should return `PipelineError::Config` if `work_dir` is `None`.

3. **Output file might not exist** after agent execution -- the agent might have failed or written to the wrong path. This is treated as a parse failure and triggers retry.

4. **Large output files** -- reading the entire file into memory for parsing. This is acceptable; LLM outputs are typically small (KB to low MB).

5. **Dry-run mode** -- in dry-run, the agent doesn't actually run, so there's no output file to parse. `StructuredOutputBuilder` should detect dry-run and return early (skip parsing), similar to how `AgentBuilder` handles it. But we can't return a `T` without actual data. Options:
   - Return a `PipelineError::Config("cannot use structured output in dry-run mode")`
   - Return `StructuredOutput` with a `None` data field (changes the ergonomics)
   - Just let it fail on "file not found" which is confusing

   Best option: detect dry-run and return a descriptive error. Users who need dry-run can use `AgentBuilder` directly.

6. **`serde` not already in deps** -- need to add it. Currently only `serde_json` is listed but `serde` itself (with `derive`) is needed for `DeserializeOwned`. Actually, `serde_json` re-exports `serde::de::DeserializeOwned` but it's better to have an explicit dep.

### Critical Files for Implementation

- `/Users/tartakovsky/Projects/websmasher/pipelin3r/packages/pipelin3r/src/agent/mod.rs` -- Add `parse_output()` method to `AgentBuilder`, extract config fields for the structured builder
- `/Users/tartakovsky/Projects/websmasher/pipelin3r/packages/pipelin3r/src/error.rs` -- Add `StructuredOutputFailed` variant
- `/Users/tartakovsky/Projects/websmasher/pipelin3r/packages/pipelin3r/src/lib.rs` -- Add `structured` module declaration and public exports
- `/Users/tartakovsky/Projects/websmasher/pipelin3r/packages/pipelin3r/Cargo.toml` -- Add `serde` dependency, optional `serde_yaml`, feature gate
- `/Users/tartakovsky/Projects/websmasher/pipelin3r/packages/pipelin3r/src/structured/mod.rs` -- New file: the core `StructuredOutputBuilder`, `StructuredOutput`, `OutputFormat`, retry loop implementation (pattern reference: `agent/mod.rs` for builder style, `transform/mod.rs` for file I/O pattern)
