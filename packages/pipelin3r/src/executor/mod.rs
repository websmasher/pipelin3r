//! Pipeline executor — wraps the SDK client with authentication and dry-run support.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shedul3r_rs_sdk::{Client, ClientConfig};

use crate::agent::{AgentConfig, AgentResult};
use crate::auth::{Auth, merge_env};
use crate::error::PipelineError;
use crate::model::{ModelConfig, Provider};
use crate::task::{TaskConfig, build_task_yaml};

/// Pipeline executor that manages SDK client, authentication, and dry-run mode.
#[allow(
    clippy::disallowed_types,
    reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
)]
pub struct Executor {
    client: Client,
    base_url: String,
    default_auth: Option<Auth>,
    default_provider: Option<Provider>,
    model_config: ModelConfig,
    dry_run: Option<std::sync::Mutex<DryRunConfig>>,
    /// Auto-forwarded env vars (`CLAUDE_ACCOUNT`, `CLAUDE_CONFIG_DIR`).
    auto_env: BTreeMap<String, String>,
}

/// Configuration for dry-run capture mode.
pub(crate) struct DryRunConfig {
    /// Base directory for capture output.
    pub base_dir: PathBuf,
    /// Per-step invocation counters, keyed by step name slug.
    pub counters: BTreeMap<String, usize>,
}

/// Capture `CLAUDE_ACCOUNT` and `CLAUDE_CONFIG_DIR` from the current
/// process environment at executor construction time.
#[allow(
    clippy::disallowed_methods,
    reason = "executor init: auto-forwarding Claude env vars is core functionality"
)]
fn capture_claude_env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if let Ok(account) = std::env::var("CLAUDE_ACCOUNT") {
        let _ = env.insert(String::from("CLAUDE_ACCOUNT"), account);
    }
    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let _ = env.insert(String::from("CLAUDE_CONFIG_DIR"), config_dir);
    }
    env
}

impl Executor {
    /// Create a new executor with the given SDK client configuration.
    ///
    /// # Errors
    /// Returns an error if the SDK client cannot be built.
    pub fn new(config: &ClientConfig) -> Result<Self, PipelineError> {
        let base_url = config.base_url.clone();
        let client = Client::new(config.clone())?;
        let auto_env = capture_claude_env();
        Ok(Self {
            client,
            base_url,
            default_auth: None,
            default_provider: None,
            model_config: ModelConfig::default_config(),
            dry_run: None,
            auto_env,
        })
    }

    /// Create a new executor with default SDK configuration.
    ///
    /// # Errors
    /// Returns an error if the SDK client cannot be built.
    pub fn with_defaults() -> Result<Self, PipelineError> {
        Self::new(&ClientConfig::default())
    }

    /// Set the default authentication for all agent invocations.
    #[must_use]
    pub fn with_default_auth(mut self, auth: Auth) -> Self {
        self.default_auth = Some(auth);
        self
    }

    /// Set the default LLM provider for all agent invocations.
    #[must_use]
    pub fn with_default_provider(mut self, provider: Provider) -> Self {
        self.default_provider = Some(provider);
        self
    }

    /// Override the model ID configuration.
    ///
    /// By default, the executor uses the embedded `models.toml` configuration.
    /// Use this to load custom model ID mappings from a different TOML source.
    #[must_use]
    pub fn with_model_config(mut self, config: ModelConfig) -> Self {
        self.model_config = config;
        self
    }

    /// Enable dry-run mode: capture prompts and task definitions to disk
    /// instead of making HTTP calls.
    #[must_use]
    #[allow(
        clippy::disallowed_types,
        reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
    )]
    pub fn with_dry_run(mut self, capture_dir: PathBuf) -> Self {
        self.dry_run = Some(std::sync::Mutex::new(DryRunConfig {
            base_dir: capture_dir,
            counters: BTreeMap::new(),
        }));
        self
    }

    /// Execute a single agent task.
    ///
    /// Handles: task YAML generation, auth/env merging, work-dir transport
    /// (local path or remote bundle upload/download), dry-run capture,
    /// and expected output verification.
    ///
    /// # Errors
    /// Returns an error if validation, YAML building, or execution fails.
    #[allow(
        clippy::too_many_lines,
        reason = "agent execution has sequential phases (validate, build YAML, merge env, log, execute, log result) that are clearer kept together"
    )]
    pub async fn run_agent(&self, config: &AgentConfig) -> Result<AgentResult, PipelineError> {
        use crate::agent::{
            execute_dry_run_capture, execute_with_work_dir, format_duration, validate_work_dir,
        };

        // Log the config so misconfigurations are visible immediately.
        tracing::info!(
            name = %config.name,
            work_dir = ?config.work_dir,
            tools = ?config.tools,
            model = ?config.model,
            execution_timeout = ?config.execution_timeout,
            provider_id = ?config.provider_id,
            max_concurrent = ?config.max_concurrent,
            expect_outputs = ?config.expect_outputs,
            prompt_len = config.prompt.len(),
            "run_agent"
        );

        // Validate work_dir before any work happens.
        if let Some(ref dir) = config.work_dir {
            validate_work_dir(dir)?;
        }

        let model_str = self.resolve_model_string(config);
        let timeout_str = config.execution_timeout.map(format_duration);

        // Build tools string from Vec<String>.
        let tools_str = config.tools.as_ref().map(|tools| {
            tools
                .iter()
                .enumerate()
                .fold(String::new(), |mut acc, (i, t)| {
                    if i > 0 {
                        acc.push(',');
                    }
                    acc.push_str(t);
                    acc
                })
        });

        // Build retry fields for task YAML.
        let max_retries = config.retry.as_ref().map(|r| r.max_retries);
        let retry_initial_delay = config
            .retry
            .as_ref()
            .map(|r| format_duration(r.initial_delay));
        let retry_backoff = config.retry.as_ref().map(|r| r.backoff_multiplier);
        let retry_max_delay = config.retry.as_ref().map(|r| format_duration(r.max_delay));

        let task_yaml = build_task_yaml(&TaskConfig {
            name: config.name.clone(),
            model: model_str,
            timeout: timeout_str,
            provider_id: config.provider_id.clone(),
            max_concurrent: config.max_concurrent,
            max_wait: config.max_wait.map(format_duration),
            max_retries,
            allowed_tools: tools_str,
            retry_initial_delay,
            retry_backoff_multiplier: retry_backoff,
            retry_max_delay,
        })?;

        // Resolve auth: config override > executor default > empty.
        let auth = config.auth.as_ref().or(self.default_auth.as_ref());
        let auth_env = auth.map(Auth::to_env).transpose()?.unwrap_or_default();

        // Merge envs: auto_env (base) + auth_env + config.env (highest priority).
        let mut merged = self.auto_env.clone();
        for (k, v) in &auth_env {
            let _ = merged.insert(k.clone(), v.clone());
        }
        let env = merge_env(merged, config.env.as_ref());

        // Dry-run: capture to disk.
        if let Some(dry_run_mutex) = self.dry_run_config() {
            return execute_dry_run_capture(
                dry_run_mutex,
                &task_yaml,
                &config.prompt,
                config.work_dir.as_deref(),
                env.as_ref(),
            );
        }

        // Execute via the work-dir transport helper.
        let result = execute_with_work_dir(
            self.sdk_client(),
            !self.is_local(),
            &task_yaml,
            &config.prompt,
            config.work_dir.as_deref(),
            &config.expect_outputs,
            env,
        )
        .await;

        // Log the outcome so failures are immediately visible.
        match &result {
            Ok(r) if r.success => {
                tracing::info!(
                    name = %config.name,
                    output_files = r.output_files.len(),
                    output_len = r.output.len(),
                    "agent succeeded"
                );
            }
            Ok(r) => {
                tracing::warn!(
                    name = %config.name,
                    output_preview = %crate::utils::truncate_str(&r.output, 200),
                    "agent failed (task returned success=false)"
                );
            }
            Err(e) => {
                tracing::error!(
                    name = %config.name,
                    error = %e,
                    "agent execution error"
                );
            }
        }

        result
    }

    /// Resolve the model string for task YAML.
    fn resolve_model_string(&self, config: &AgentConfig) -> Option<String> {
        config.model.as_ref().map(|m| {
            let provider = self.default_provider.clone().unwrap_or_default();
            self.model_config.resolve(m, &provider)
        })
    }

    /// Get a reference to the underlying SDK client.
    pub(crate) const fn sdk_client(&self) -> &Client {
        &self.client
    }

    /// Get the default provider, if set.
    #[cfg(test)]
    pub(crate) const fn default_provider(&self) -> Option<&Provider> {
        self.default_provider.as_ref()
    }

    /// Get the auto-forwarded environment variables.
    #[cfg(test)]
    pub(crate) const fn auto_env(&self) -> &BTreeMap<String, String> {
        &self.auto_env
    }

    /// Get the dry-run config mutex, if dry-run is enabled.
    #[allow(
        clippy::disallowed_types,
        reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
    )]
    pub(crate) const fn dry_run_config(&self) -> Option<&std::sync::Mutex<DryRunConfig>> {
        self.dry_run.as_ref()
    }

    /// Whether the shedul3r server is on the local machine (shared filesystem).
    ///
    /// Returns `true` when the base URL points to `localhost` or `127.0.0.1`,
    /// meaning work directories can be passed as paths directly. When `false`,
    /// the executor uploads work directory contents via bundle endpoints and
    /// downloads outputs after execution.
    pub(crate) fn is_local(&self) -> bool {
        // Strip scheme to get the host portion.
        let after_scheme = self
            .base_url
            .strip_prefix("http://")
            .or_else(|| self.base_url.strip_prefix("https://"))
            .unwrap_or(&self.base_url);

        // Strip optional `user:pass@` credentials (RFC 3986 userinfo).
        let after_creds = after_scheme.find('@').map_or(after_scheme, |pos| {
            after_scheme
                .get(pos.saturating_add(1)..)
                .unwrap_or(after_scheme)
        });

        // Hostnames are case-insensitive per RFC 2616.
        let host_part = after_creds.to_ascii_lowercase();

        is_local_host(&host_part, "localhost")
            || is_local_host(&host_part, "127.0.0.1")
            || is_local_host(&host_part, "[::1]")
    }
}

/// Check whether `url_after_scheme` starts with `host` and is followed by
/// a port separator (`:`), a path separator (`/`), or end-of-string.
///
/// This prevents subdomain bypasses such as `localhost.evil.com` being
/// classified as local.
fn is_local_host(url_after_scheme: &str, host: &str) -> bool {
    if let Some(rest) = url_after_scheme.strip_prefix(host) {
        rest.is_empty() || rest.starts_with(':') || rest.starts_with('/')
    } else {
        false
    }
}

/// Extract a step name slug from the task YAML's `name:` field.
///
/// Falls back to `"unknown"` if no name field is found.
pub(crate) fn extract_step_name(task_yaml: &str) -> String {
    for line in task_yaml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name:") {
            let name = rest.trim();
            if !name.is_empty() {
                let raw_slug: String = name
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect();
                // Collapse consecutive dashes.
                let mut slug = String::with_capacity(raw_slug.len());
                let mut prev_dash = false;
                for c in raw_slug.chars() {
                    if c == '-' {
                        if !prev_dash {
                            slug.push(c);
                        }
                        prev_dash = true;
                    } else {
                        slug.push(c);
                        prev_dash = false;
                    }
                }
                let trimmed_slug = slug.trim_matches('-').to_owned();
                if !trimmed_slug.is_empty() {
                    return trimmed_slug;
                }
            }
        }
    }
    String::from("unknown")
}

#[cfg(test)]
mod tests;
