# pipelin3r API v2: Config Structs + Functions (Post-Adversarial)

**Date:** 2026-03-17 19:19
**Task:** Final API design incorporating all adversarial findings from both steady-parent and dev-process reviews.

## Changes from v1

1. `run_pool_map` ownership fixed — closure returns `(T, R)`, framework collects
2. `AgentConfig` gets scheduling fields (provider_id, max_concurrent, max_wait, retry)
3. `env` field added to `AgentConfig`
4. File-poll recovery wired through from SDK
5. `AgentConfig::new(name, prompt)` constructor, no `Default`
6. `Tool` is `Vec<String>` not an enum
7. Timeouts split: `execution_timeout` (task YAML) vs `request_timeout` (HTTP)
8. `expect_outputs` verified after execution, contents returned in result
9. `BundleDir` RAII utility for ephemeral work directories
10. Utility functions: `strip_preamble`, `parse_labeled_fields`, `chunk_by_size`
11. Executor auto-forwards `CLAUDE_ACCOUNT`/`CLAUDE_CONFIG_DIR` env vars

## 1. Agent Execution

### AgentConfig

```rust
/// Configuration for a single agent invocation.
///
/// Required fields (`name`, `prompt`) are set via the constructor.
/// Optional fields use struct update syntax from a defaults instance.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    // ── Required (set via constructor) ──
    /// Step name for logging and dry-run capture.
    pub name: String,
    /// Prompt text sent as stdin to the agent subprocess.
    pub prompt: String,

    // ── Agent settings ──
    /// LLM model to use.
    pub model: Option<Model>,
    /// Work directory path (the workspace channel).
    pub work_dir: Option<PathBuf>,
    /// Agent execution timeout (goes into task YAML `timeout:` field).
    pub execution_timeout: Option<Duration>,
    /// Allowed tools for the agent (e.g., ["Read", "Write", "Bash"]).
    /// None = all tools allowed.
    pub tools: Option<Vec<String>>,
    /// Auth override (falls back to executor default).
    pub auth: Option<Auth>,
    /// Additional environment variables for the subprocess.
    pub env: Option<BTreeMap<String, String>>,

    // ── Scheduling settings (go into task YAML for shedul3r) ──
    /// Provider/limiter key for rate limiting grouping (e.g., "claude").
    pub provider_id: Option<String>,
    /// Maximum concurrent tasks for this provider key.
    pub max_concurrent: Option<usize>,
    /// Maximum time to wait in shedul3r's queue.
    pub max_wait: Option<Duration>,
    /// Retry configuration for failed executions.
    pub retry: Option<RetryConfig>,

    // ── Output settings ──
    /// Expected output files (relative to work_dir).
    /// After execution: verified to exist, downloaded if remote.
    /// Contents returned in AgentResult.output_files.
    pub expect_outputs: Vec<String>,

    // ── HTTP transport ──
    /// HTTP request timeout to shedul3r (must be > execution_timeout + max_wait).
    /// Default: 45 minutes.
    pub request_timeout: Option<Duration>,
}

/// Retry configuration for agent tasks.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub backoff_multiplier: f64,
    pub max_delay: Duration,
}

impl AgentConfig {
    /// Create a new config with required fields.
    /// All optional fields are None/empty.
    pub fn new(name: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            prompt: prompt.into(),
            model: None,
            work_dir: None,
            execution_timeout: None,
            tools: None,
            auth: None,
            env: None,
            provider_id: None,
            max_concurrent: None,
            max_wait: None,
            retry: None,
            expect_outputs: Vec::new(),
            request_timeout: None,
        }
    }
}
```

### AgentResult

```rust
/// Result of an agent invocation.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// Whether the agent completed successfully (exit code 0).
    pub success: bool,
    /// Agent response text (stdout from shedul3r).
    pub output: String,
    /// Contents of expected output files (filename → content).
    /// Populated only for files listed in `expect_outputs` that exist after execution.
    pub output_files: BTreeMap<String, String>,
    /// Execution metadata from shedul3r.
    pub metadata: ExecutionMetadata,
}

impl AgentResult {
    pub fn require_success(&self) -> Result<&Self, PipelineError> { ... }
}
```

### Executor

```rust
impl Executor {
    /// Execute a single agent task.
    ///
    /// Handles: task YAML generation, auth/env merging, work_dir transport
    /// (local path or remote bundle upload/download), file-poll recovery
    /// for long-running tasks, dry-run capture, and expected output verification.
    pub async fn run_agent(&self, config: &AgentConfig) -> Result<AgentResult, PipelineError> { ... }
}
```

**Internally, `run_agent`:**
1. Validates work_dir (if set)
2. Builds task YAML from config (name, model, tools, execution_timeout, provider_id, max_concurrent, max_wait, retry)
3. Merges env vars: executor defaults (CLAUDE_ACCOUNT, CLAUDE_CONFIG_DIR) + config.env + auth.to_env()
4. If dry-run: capture to disk, return
5. If remote (non-localhost): upload work_dir as bundle
6. Submit task to shedul3r via SDK
7. If `expect_outputs` is non-empty: use `submit_task_with_recovery` (file-poll race)
8. If remote: download expected output files back to work_dir
9. Read expected output files into `output_files` map
10. Clean up remote bundle
11. Return AgentResult

### Usage

```rust
// Reusable defaults for a pipeline
let defaults = AgentConfig::new("", "") // name/prompt overridden per step
    .with_model(Model::Sonnet4_6)
    .with_execution_timeout(Duration::from_secs(600))
    .with_tools(vec!["Read", "Write"])
    .with_provider_id("claude")
    .with_max_concurrent(3)
    .with_max_wait(Duration::from_secs(1800))
    .with_retry(RetryConfig {
        max_retries: 2,
        initial_delay: Duration::from_secs(5),
        backoff_multiplier: 2.0,
        max_delay: Duration::from_secs(30),
    });

// Per-step config
let config = AgentConfig {
    name: String::from("write-article"),
    prompt: filled_prompt,
    work_dir: Some(dir.path().to_path_buf()),
    expect_outputs: vec![String::from("article.mdx")],
    ..defaults.clone()
};

let result = executor.run_agent(&config).await?;
result.require_success()?;

// Output file content is in the result
let article = result.output_files.get("article.mdx")
    .ok_or_else(|| PipelineError::Other("article.mdx not produced".into()))?;
```

## 2. Batch Execution

### run_pool_map

```rust
/// Execute an async function for each item with bounded concurrency.
///
/// The closure receives (item, index, total) and must return (item, result).
/// The item goes IN to the closure and comes BACK OUT paired with the result,
/// solving the ownership problem without requiring Clone.
pub async fn run_pool_map<T, F, Fut, R>(
    items: Vec<T>,
    concurrency: usize,
    total: usize,  // explicit total (may differ from items.len() after filtering)
    f: F,
) -> Vec<(T, Result<R, PipelineError>)>
where
    T: Send + 'static,
    F: Fn(T, usize, usize) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = (T, Result<R, PipelineError>)> + Send,
    R: Send + 'static;
```

**Ownership solved:** The closure takes `T` by value and returns `(T, Result<R>)`. The item goes through the closure and comes back. No Clone needed.

**`total` is explicit:** Not derived from `items.len()`, because the caller may have filtered items and wants progress to show the original count.

### Usage

```rust
// Filter first (standard Rust)
let all_articles = discover_articles(&research_dir)?;
let pending: Vec<_> = all_articles.iter()
    .filter(|a| !a.output_path().exists())
    .cloned()
    .collect();
let total = all_articles.len();  // show progress against full count

// Map with concurrency
let results = run_pool_map(pending, 4, total, |article, idx, total| {
    let exec = executor.clone();
    let defaults = defaults.clone();
    async move {
        tracing::info!("[{}/{total}] {}", idx + 1, article.slug);
        let config = AgentConfig {
            name: format!("write-{}", article.slug),
            prompt: fill_prompt(&article),
            work_dir: Some(article.work_dir.clone()),
            expect_outputs: vec![String::from("article.mdx")],
            ..defaults
        };
        let result = exec.run_agent(&config).await;
        (article, result)  // item comes back out
    }
}).await;

// Reduce (standard Rust)
let succeeded: Vec<_> = results.into_iter()
    .filter_map(|(article, r)| r.ok().map(|res| (article, res)))
    .collect();
let merged = merge_articles(succeeded);
```

## 3. BundleDir (RAII ephemeral work directory)

```rust
/// RAII guard for an ephemeral work directory.
///
/// Creates a `.bundle-{slug}` directory under the given parent.
/// Removes the directory on drop (even on panic/error).
pub struct BundleDir {
    path: PathBuf,
}

impl BundleDir {
    /// Create a new bundle directory.
    pub fn new(parent: &Path, slug: &str) -> Result<Self, PipelineError> { ... }

    /// Get the path to the bundle directory.
    pub fn path(&self) -> &Path { &self.path }
}

impl Drop for BundleDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
```

### Usage

```rust
let bundle = BundleDir::new(&output_dir, &article.slug)?;
std::fs::write(bundle.path().join("sources.md"), &sources)?;
std::fs::write(bundle.path().join("prompt.md"), &prompt)?;

let config = AgentConfig {
    name: format!("write-{}", article.slug),
    prompt: String::from("Read prompt.md and follow the instructions"),
    work_dir: Some(bundle.path().to_path_buf()),
    expect_outputs: vec![String::from("article.mdx")],
    ..defaults.clone()
};

let result = executor.run_agent(&config).await?;
// bundle dir is cleaned up when `bundle` goes out of scope
```

## 4. Command Execution

```rust
/// Configuration for a shell command.
#[derive(Debug, Clone)]
pub struct CommandConfig {
    pub program: String,
    pub args: Vec<String>,
    pub work_dir: Option<PathBuf>,
    pub env: Option<BTreeMap<String, String>>,
    pub timeout: Option<Duration>,
}

impl CommandConfig {
    pub fn new(program: impl Into<String>) -> Self { ... }
}

/// Execute a shell command.
pub async fn run_command(config: &CommandConfig) -> Result<CommandResult, PipelineError> { ... }
```

## 5. Image Generation

```rust
/// Shared HTTP/rate-limit config for image generation.
pub struct ImageGenHttpConfig { ... }

/// Configuration for a single image generation call.
#[derive(Debug, Clone)]
pub struct ImageGenConfig {
    pub model: ImageModel,
    pub prompt: String,
    pub reference_images: Vec<RefImage>,
    pub aspect_ratio: AspectRatio,
    pub work_dir: PathBuf,
    pub output_filename: String,
}

impl ImageGenConfig {
    pub fn new(prompt: impl Into<String>, work_dir: PathBuf) -> Self { ... }
}

/// Generate an image via OpenRouter API.
pub async fn generate_image(
    http: &ImageGenHttpConfig,
    config: &ImageGenConfig,
) -> Result<ImageGenResult, PipelineError> { ... }
```

## 6. Validate-and-Fix Loop

```rust
/// Configuration for a validate-fix loop.
#[derive(Debug, Clone)]
pub struct ValidateConfig {
    pub name: String,
    pub work_dir: PathBuf,
    pub max_iterations: u32,
    /// Agent config defaults for AgentFix remediation actions.
    pub fix_agent_defaults: AgentConfig,
}

impl ValidateConfig {
    pub fn new(name: impl Into<String>, work_dir: PathBuf) -> Self { ... }
}

/// Run a validate-fix loop.
pub async fn validate_and_fix<V, S>(
    executor: &Executor,
    config: &ValidateConfig,
    validator: V,
    strategy: S,
) -> Result<ValidateResult, PipelineError>
where
    V: AsyncValidator,
    S: Fn(&ValidationReport, u32) -> Vec<RemediationAction> + Send + Sync;
```

## 7. Utility Functions

```rust
/// Strip outermost code fences from text.
pub fn strip_code_fences(text: &str) -> &str { ... }

/// Strip preamble text before the first occurrence of any marker pattern.
/// Markers can be strings like "{", "---", "<Blog", etc.
pub fn strip_preamble<'a>(text: &'a str, markers: &[&str]) -> &'a str { ... }

/// Parse labeled fields from LLM output.
/// Extracts text after labels like "SCENE:", "CAPTION:", "ALT:" etc.
pub fn parse_labeled_fields<'a>(text: &'a str, labels: &[&str]) -> BTreeMap<&'a str, &'a str> { ... }

/// Split items into chunks where each chunk's estimated size is under max_size.
pub fn chunk_by_size<T>(
    items: Vec<T>,
    max_size: usize,
    size_fn: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> { ... }
```

## 8. Executor Changes

```rust
pub struct Executor {
    client: Client,
    base_url: String,
    default_auth: Option<Auth>,
    default_provider: Option<Provider>,
    model_config: ModelConfig,
    dry_run: Option<Mutex<DryRunConfig>>,
    /// Auto-forwarded env vars (CLAUDE_ACCOUNT, CLAUDE_CONFIG_DIR).
    auto_env: BTreeMap<String, String>,
}

impl Executor {
    pub fn new(config: &ClientConfig) -> Result<Self, PipelineError> {
        // ... existing setup ...
        // Auto-capture Claude env vars at construction time
        let auto_env = capture_claude_env();
        // ...
    }
}
```

The `auto_env` is merged into every agent task's environment automatically. The user's `config.env` overrides auto_env values.

## Files to Create/Modify

### New files:
- `src/bundle_dir.rs` — BundleDir RAII type
- `src/utils.rs` — strip_code_fences, strip_preamble, parse_labeled_fields, chunk_by_size
- `src/image_gen/mod.rs` — generate_image, configs, types
- `src/image_gen/client.rs` — OpenRouter HTTP client
- `src/image_gen/types.rs` — AspectRatio, ImageModel, RefImage
- `src/validate/mod.rs` — validate_and_fix, ValidationReport, RemediationAction
- `src/validate/report.rs` — ValidationReport, ValidationFinding

### Modified files:
- `src/agent/mod.rs` — replace AgentBuilder with AgentConfig, add run_agent to Executor
- `src/agent/execute.rs` — adapt to take &AgentConfig, wire file-poll recovery
- `src/command/mod.rs` — replace CommandBuilder with CommandConfig + run_command
- `src/pool/mod.rs` — add run_pool_map
- `src/executor/mod.rs` — add auto_env, run_agent method
- `src/task/mod.rs` — extend YAML generation for provider_id, max_concurrent, max_wait, retry
- `src/lib.rs` — update exports
- `src/error.rs` — add new variants
- `Cargo.toml` — add serde, reqwest, limit3r, base64 deps
- `tests/` — rewrite all tests
