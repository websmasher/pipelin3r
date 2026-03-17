//! Pipeline executor — wraps the SDK client with authentication and dry-run support.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shedul3r_rs_sdk::{Client, ClientConfig};

use crate::agent::AgentBuilder;
use crate::auth::Auth;
use crate::command::CommandBuilder;
use crate::error::PipelineError;
use crate::model::{ModelConfig, Provider};
use crate::transform::TransformBuilder;

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
}

/// Configuration for dry-run capture mode.
pub(crate) struct DryRunConfig {
    /// Base directory for capture output.
    pub base_dir: PathBuf,
    /// Per-step invocation counters, keyed by step name slug.
    pub counters: BTreeMap<String, usize>,
}

impl Executor {
    /// Create a new executor with the given SDK client configuration.
    ///
    /// # Errors
    /// Returns an error if the SDK client cannot be built.
    pub fn new(config: &ClientConfig) -> Result<Self, PipelineError> {
        let base_url = config.base_url.clone();
        let client = Client::new(config.clone())?;
        Ok(Self {
            client,
            base_url,
            default_auth: None,
            default_provider: None,
            model_config: ModelConfig::default_config(),
            dry_run: None,
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

    /// Create an agent builder for a named agent.
    pub fn agent(&self, name: &str) -> AgentBuilder<'_> {
        AgentBuilder::new(self, name)
    }

    /// Create a command builder for the given program.
    pub fn command(&self, program: &str) -> CommandBuilder {
        CommandBuilder::new(program)
    }

    /// Create a transform builder for the given step name.
    pub fn transform(&self, name: &str) -> TransformBuilder {
        TransformBuilder::new(name)
    }

    /// Get a reference to the underlying SDK client.
    pub(crate) const fn sdk_client(&self) -> &Client {
        &self.client
    }

    /// Get the default auth, if set.
    pub(crate) const fn default_auth(&self) -> Option<&Auth> {
        self.default_auth.as_ref()
    }

    /// Get the default provider, if set.
    pub(crate) const fn default_provider(&self) -> Option<&Provider> {
        self.default_provider.as_ref()
    }

    /// Get the model configuration.
    pub(crate) const fn model_config(&self) -> &ModelConfig {
        &self.model_config
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
