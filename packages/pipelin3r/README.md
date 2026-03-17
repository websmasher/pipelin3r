# pipelin3r

Pipeline orchestration for LLM-powered workflows.

## Installation

```sh
cargo add pipelin3r
```

## Quick Start

```rust
use pipelin3r::{Executor, Model, Auth};
use std::time::Duration;

let executor = Executor::with_defaults()?
    .with_default_auth(Auth::ApiKey(String::from("sk-...")))
    .with_default_provider(pipelin3r::Provider::Anthropic);

let result = executor
    .agent("summarize")
    .model(Model::Sonnet4_6)
    .timeout(Duration::from_secs(300))
    .prompt("Summarize the following document...")
    .execute()
    .await?;

result.require_success()?;
println!("{}", result.output);
```

### Batch execution with bounded concurrency

```rust
use pipelin3r::{Executor, Model, AgentTask};

let items = vec!["doc1.txt", "doc2.txt", "doc3.txt"];

let results = executor
    .agent("summarize-batch")
    .model(Model::Sonnet4_6)
    .items(items, 2) // concurrency = 2
    .for_each(|path| {
        AgentTask::new()
            .prompt(&format!("Summarize {path}"))
    })
    .execute()
    .await?;
```

## Features

- **Agent builder** -- single invocation or batch with bounded concurrency via `run_pool`
- **Per-invocation auth injection** -- OAuth, API key, or environment variable authentication
- **Injection-safe template filler** -- two-phase, single-pass content replacement
- **Model/Provider enums** -- TOML-configurable model ID resolution per provider
- **Work directory transport** -- auto-detected local path or remote bundle upload/download
- **Shell command execution** -- run shell commands as pipeline steps
- **File transform** -- pure Rust functions on file data
- **Dry-run capture mode** -- write prompts and task YAML to disk for testing without HTTP calls
- **Typed errors** -- `PipelineError` enum (no `anyhow`)

## License

MIT
