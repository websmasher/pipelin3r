# PipelineContext: file routing between steps

**Date:** 2026-03-18 12:16
**Task:** Add PipelineContext to pipelin3r that manages input/output file routing between steps, handling local vs remote transport automatically.

## Goal

Pipeline authors declare `inputs` (files this step reads) and `outputs` (files this step writes) per step. The context verifies inputs exist, uploads only needed files for remote execution, downloads outputs back, and verifies they were produced.

## API

```rust
let mut ctx = PipelineContext::new(&executor, package_dir);

// LLM step: agent runs, context handles file transport
ctx.run_agent(AgentStep {
    config: AgentConfig { name, prompt, .. },
    inputs: vec![],  // no input files needed
    outputs: vec!["architecture.md"],
}).await?;

// Batch LLM step
ctx.run_agent_batch(items, concurrency, |item| AgentStep {
    config: AgentConfig { name, prompt, .. },
    inputs: vec![format!("architecture.md")],
    outputs: vec![format!("research/{}/overview.md", item.slug)],
}).await?;

// Programmatic step: runs locally, no agent
ctx.run_local("extract-tests", |dir| {
    // dir is the package_dir, files are on disk
    clone_and_extract(dir, "tests");
})?;
```

## What PipelineContext does

### For `run_agent`:
1. Verify all `inputs` files exist at `package_dir/{path}`
2. If local: set work_dir to package_dir, set expect_outputs from `outputs`
3. If remote: upload only `inputs` files as bundle, run agent, download `outputs` files back to package_dir
4. Verify all `outputs` files exist after execution

### For `run_agent_batch`:
1. For each item: same as run_agent but per-item inputs/outputs
2. Bounded concurrency via run_pool_map
3. Collect per-item results

### For `run_local`:
1. Call the closure with package_dir
2. That's it — local steps manage their own files

## Key design decisions

- `inputs` and `outputs` are Vec<String> — relative paths from package_dir
- The context does NOT track what files exist across steps. It checks the filesystem each time.
- For remote: only `inputs` files are uploaded (not the whole package_dir). This solves the multipart upload size problem.
- For local: inputs are just verified to exist. No copying.
- `run_local` is synchronous — no async needed for programmatic steps.

## Implementation

### New type: AgentStep
```rust
pub struct AgentStep {
    pub config: AgentConfig,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}
```

### New type: PipelineContext
```rust
pub struct PipelineContext {
    executor: Arc<Executor>,
    base_dir: PathBuf,
}
```

### Methods
- `new(executor, base_dir)` — constructor
- `run_agent(&self, step: AgentStep) -> Result<AgentResult>` — single agent with file routing
- `run_agent_batch<T>(&self, items, concurrency, f) -> Vec<Result<AgentResult>>` — batch with per-item file routing
- `run_local<F>(&self, name, f) -> Result<()>` — programmatic step

## Files to create/modify

- `packages/pipelin3r/src/pipeline.rs` — PipelineContext, AgentStep
- `packages/pipelin3r/src/lib.rs` — add module + exports
