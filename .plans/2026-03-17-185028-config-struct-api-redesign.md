# pipelin3r API Redesign: Config Structs + Functions

**Date:** 2026-03-17 18:50
**Task:** Replace builder/chain pattern with config structs + plain functions across the entire pipelin3r API.

## Goal

Every operation in pipelin3r becomes: construct a config struct → call a function. No builders, no chaining, no `.execute()`. The pipeline author writes regular Rust code between calls.

## Motivation

Builder chains are fragile:
- Forgotten `.execute()` silently drops work
- Intermediate state is invisible (can't log/inspect config)
- Hard to conditionally set fields
- Can't serialize, store, or reuse configs
- Forces a predicted sequence

Config structs + functions are explicit:
- All params visible at construction
- Can log/serialize the full config
- Conditional fields are just `if` statements
- Configs can be stored, cloned, passed around, reused
- No "forgotten execute" — function calls either happen or don't

## API Surface

### 1. Agent Execution

```rust
/// Configuration for a single agent invocation.
#[derive(Debug, Clone, Default)]
pub struct AgentConfig {
    /// Step name for logging and dry-run capture.
    pub name: String,
    /// LLM model to use.
    pub model: Option<Model>,
    /// Prompt text (the message channel).
    pub prompt: String,
    /// Work directory path (the workspace channel).
    pub work_dir: Option<PathBuf>,
    /// Execution timeout.
    pub timeout: Option<Duration>,
    /// Allowed tools for the agent.
    pub tools: Option<Vec<Tool>>,
    /// Auth override (falls back to executor default).
    pub auth: Option<Auth>,
    /// Expected output files for remote download.
    pub expect_outputs: Vec<String>,
}

/// Result of an agent invocation.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub success: bool,
    pub output: String,
    pub metadata: ExecutionMetadata,
}

impl AgentResult {
    pub fn require_success(&self) -> Result<&Self, PipelineError> { ... }
}

/// Execute a single agent task.
impl Executor {
    pub async fn run_agent(&self, config: &AgentConfig) -> Result<AgentResult, PipelineError> { ... }
}
```

**Usage:**

```rust
let config = AgentConfig {
    name: String::from("write-article"),
    model: Some(Model::Sonnet4_6),
    prompt: filled_prompt,
    work_dir: Some(dir.path().to_path_buf()),
    timeout: Some(Duration::from_secs(600)),
    tools: Some(vec![Tool::Read, Tool::Write]),
    ..AgentConfig::default()
};

let result = executor.run_agent(&config).await?;
result.require_success()?;
let article = std::fs::read_to_string(dir.path().join("article.mdx"))?;
```

**Reusable defaults:**

```rust
let defaults = AgentConfig {
    model: Some(Model::Sonnet4_6),
    timeout: Some(Duration::from_secs(600)),
    tools: Some(vec![Tool::Read, Tool::Write]),
    ..AgentConfig::default()
};

// Per-step: clone defaults, set step-specific fields
let config = AgentConfig {
    name: String::from("write-article"),
    prompt: fill_prompt(&article),
    work_dir: Some(article_dir.clone()),
    ..defaults.clone()
};
```

### 2. Batch Execution

```rust
/// Execute an async function for each item with bounded concurrency.
/// Returns (item, result) pairs preserving item identity.
pub async fn run_pool_map<T, F, Fut, R>(
    items: Vec<T>,
    concurrency: usize,
    f: F,
) -> Vec<(T, Result<R, PipelineError>)>
where
    T: Send + 'static,
    F: Fn(T, usize, usize) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<R, PipelineError>> + Send,
    R: Send + 'static;
```

**Usage:**

```rust
let items: Vec<Language> = vec![...];
let results = run_pool_map(items, 4, |lang, idx, total| {
    let exec = executor.clone();
    async move {
        tracing::info!("[{}/{total}] {lang}", idx + 1);
        let config = AgentConfig {
            name: format!("find-libs-{lang}"),
            prompt: format!("Find {lang} libraries"),
            work_dir: Some(prepare_dir(&lang)?),
            ..defaults.clone()
        };
        exec.run_agent(&config).await
    }
}).await;

let (ok, fail): (Vec<_>, Vec<_>) = results.into_iter().partition(|(_, r)| r.is_ok());
```

### 3. Command Execution

```rust
/// Configuration for a shell command.
#[derive(Debug, Clone, Default)]
pub struct CommandConfig {
    /// Program to execute.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Work directory.
    pub work_dir: Option<PathBuf>,
    /// Environment variables.
    pub env: Option<BTreeMap<String, String>>,
    /// Timeout.
    pub timeout: Option<Duration>,
}

/// Result of a command execution.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Execute a shell command.
pub async fn run_command(config: &CommandConfig) -> Result<CommandResult, PipelineError> { ... }
```

### 4. Image Generation

```rust
/// Shared HTTP/rate-limit config for image generation.
#[derive(Debug, Clone)]
pub struct ImageGenHttpConfig {
    pub api_key: String,
    pub rate_limit: Option<limit3r::RateLimitConfig>,
    pub timeout: Option<Duration>,
}

/// Configuration for a single image generation call.
#[derive(Debug, Clone, Default)]
pub struct ImageGenConfig {
    pub model: ImageModel,
    pub prompt: String,
    pub reference_images: Vec<RefImage>,
    pub aspect_ratio: AspectRatio,
    pub image_size: ImageSize,
    pub work_dir: PathBuf,
    pub output_filename: String,
}

/// Result of image generation.
#[derive(Debug, Clone)]
pub struct ImageGenResult {
    pub success: bool,
    pub output_files: Vec<PathBuf>,
    pub cost: Option<f64>,
    pub output_mime: Option<String>,
}

/// Generate an image via OpenRouter API.
pub async fn generate_image(
    http: &ImageGenHttpConfig,
    config: &ImageGenConfig,
) -> Result<ImageGenResult, PipelineError> { ... }
```

**Usage (two-phase scene → image):**

```rust
// Phase 1: scene description via agent
let scene_config = AgentConfig {
    name: String::from("scene"),
    prompt: format!("Describe a scene for '{}'", chapter.title),
    work_dir: Some(chapter_dir.clone()),
    ..defaults.clone()
};
let scene_result = executor.run_agent(&scene_config).await?;
let scene_text = std::fs::read_to_string(chapter_dir.join("scene.txt"))?;

// Phase 2: image generation via API
let image_config = ImageGenConfig {
    model: ImageModel::Gemini3_1Flash,
    prompt: scene_text,
    reference_images: vec![style_ref],
    aspect_ratio: AspectRatio::Portrait2x3,
    work_dir: chapter_dir.clone(),
    output_filename: String::from("cover.png"),
    ..ImageGenConfig::default()
};
let image_result = generate_image(&http_config, &image_config).await?;
```

### 5. Utility Functions

```rust
/// Strip outermost code fences from text.
pub fn strip_code_fences(text: &str) -> String { ... }

/// Strip preamble text before the first structural character.
pub fn strip_preamble(text: &str, marker: char) -> String { ... }

/// Template filling with injection protection.
/// (TemplateFiller stays as-is — it's already a struct, not a chain)
```

### 6. Validate-and-Fix Loop

```rust
/// Configuration for a validate-fix loop.
#[derive(Debug, Clone)]
pub struct ValidateConfig {
    pub name: String,
    pub work_dir: PathBuf,
    pub max_iterations: u32,
    /// Agent config defaults for fix steps.
    pub fix_agent_defaults: AgentConfig,
}

/// Result of a validate-fix loop.
#[derive(Debug, Clone)]
pub struct ValidateResult {
    pub converged: bool,
    pub iterations: u32,
    pub final_report: ValidationReport,
    pub history: Vec<ValidationReport>,
}

/// Run a validate-fix loop.
///
/// validator: checks current state, returns findings
/// strategy: given findings, returns remediation actions
pub async fn validate_and_fix<V, S>(
    executor: &Executor,
    config: &ValidateConfig,
    validator: V,
    strategy: S,
) -> Result<ValidateResult, PipelineError>
where
    V: Fn(&Path) -> Pin<Box<dyn Future<Output = Result<ValidationReport, PipelineError>> + Send + '_>> + Send + Sync,
    S: Fn(&ValidationReport, u32) -> Vec<RemediationAction> + Send + Sync;
```

**Usage:**

```rust
let config = ValidateConfig {
    name: String::from("verify-parser"),
    work_dir: package_dir.clone(),
    max_iterations: 3,
    fix_agent_defaults: AgentConfig {
        model: Some(Model::Sonnet4_6),
        tools: Some(vec![Tool::Read, Tool::Edit, Tool::Bash]),
        ..AgentConfig::default()
    },
};

let result = validate_and_fix(
    &executor,
    &config,
    |dir| Box::pin(async move {
        let output = tokio::process::Command::new("cargo")
            .args(["test"]).current_dir(dir).output().await?;
        if output.status.success() {
            Ok(ValidationReport::pass())
        } else {
            Ok(ValidationReport::fail_raw(&String::from_utf8_lossy(&output.stderr)))
        }
    }),
    |report, _iter| {
        vec![RemediationAction::AgentFix {
            prompt: format!("Fix these test failures:\n{}", report.to_markdown()),
            work_dir_override: None,
        }]
    },
).await?;
```

## What Changes from Current Code

| Current (builder) | New (config + function) |
|---|---|
| `executor.agent("name")` | `AgentConfig { name, ... }` |
| `.model(M).prompt(p).work_dir(d)` | struct fields |
| `.execute().await?` | `executor.run_agent(&config).await?` |
| `executor.command("prog")` | `CommandConfig { program, ... }` |
| `.execute().await?` | `run_command(&config).await?` |
| `AgentBatchBuilder` | `run_pool_map(items, concurrency, f)` |
| `TransformBuilder` | stays or becomes config+fn (low priority) |

## Migration Path

This is a breaking API change. Since the crate is pre-1.0 and unpublished, that's fine. Steps:

1. Define all config structs and result types
2. Add `run_agent`, `run_command`, `generate_image`, `validate_and_fix` functions
3. Rewrite internal execution logic to work with configs
4. Remove all builder types (`AgentBuilder`, `AgentBatchBuilder`, `CommandBuilder`)
5. Update tests
6. Update README examples

## Files to Modify

- `packages/pipelin3r/src/agent/mod.rs` — replace AgentBuilder with AgentConfig + run_agent
- `packages/pipelin3r/src/agent/execute.rs` — adapt execution to take &AgentConfig
- `packages/pipelin3r/src/command/mod.rs` — replace CommandBuilder with CommandConfig + run_command
- `packages/pipelin3r/src/pool/mod.rs` — add run_pool_map
- `packages/pipelin3r/src/executor/mod.rs` — add run_agent method, remove builder factories
- `packages/pipelin3r/src/lib.rs` — update exports
- `packages/pipelin3r/src/image_gen/` — new module (config+fn from start)
- `packages/pipelin3r/src/validate/` — new module (config+fn from start)
- `packages/pipelin3r/tests/` — rewrite all tests
