//! Pipeline executor — wraps the SDK client with authentication and dry-run support.

use std::path::PathBuf;
use std::sync::Mutex;

use shedul3r_rs_sdk::{Client, ClientConfig};

use crate::agent::AgentBuilder;
use crate::auth::Auth;
use crate::command::CommandBuilder;
use crate::error::PipelineError;
use crate::model::{ModelConfig, Provider};
use crate::transform::TransformBuilder;

/// Pipeline executor that manages SDK client, authentication, and dry-run mode.
pub struct Executor {
    client: Client,
    default_auth: Option<Auth>,
    default_provider: Option<Provider>,
    model_config: ModelConfig,
    dry_run: Option<Mutex<DryRunConfig>>,
    remote: bool,
}

/// Configuration for dry-run capture mode.
pub(crate) struct DryRunConfig {
    pub base_dir: PathBuf,
    pub counter: usize,
}

impl Executor {
    /// Create a new executor with the given SDK client configuration.
    ///
    /// # Errors
    /// Returns an error if the SDK client cannot be built.
    pub fn new(config: &ClientConfig) -> Result<Self, PipelineError> {
        let client = Client::new(config.clone())?;
        Ok(Self {
            client,
            default_auth: None,
            default_provider: None,
            model_config: ModelConfig::default_config(),
            dry_run: None,
            remote: false,
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

    /// Enable remote mode: upload bundles to the server before task
    /// submission and download outputs after completion.
    #[must_use]
    pub const fn with_remote(mut self) -> Self {
        self.remote = true;
        self
    }

    /// Enable dry-run mode: capture prompts and task definitions to disk
    /// instead of making HTTP calls.
    #[must_use]
    pub fn with_dry_run(mut self, capture_dir: PathBuf) -> Self {
        self.dry_run = Some(Mutex::new(DryRunConfig {
            base_dir: capture_dir,
            counter: 0,
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
    pub(crate) const fn dry_run_config(&self) -> Option<&Mutex<DryRunConfig>> {
        self.dry_run.as_ref()
    }

    /// Whether remote bundle upload/download is enabled.
    pub(crate) const fn is_remote(&self) -> bool {
        self.remote
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
mod tests {
    use super::*;

    #[test]
    fn extract_step_name_basic() {
        let yaml = "name: 3_1_implement_tests\ncommand: echo\n";
        assert_eq!(
            extract_step_name(yaml),
            "3-1-implement-tests",
            "should slugify name field"
        );
    }

    #[test]
    fn extract_step_name_missing() {
        let yaml = "command: echo\ntimeout: 5m\n";
        assert_eq!(
            extract_step_name(yaml),
            "unknown",
            "should fallback to unknown when name is missing"
        );
    }

    #[test]
    fn extract_step_name_special_chars() {
        let yaml = "name: Hello World! (v2)\n";
        assert_eq!(
            extract_step_name(yaml),
            "hello-world-v2",
            "should replace non-alphanumeric with dashes, collapsing consecutive dashes"
        );
    }

    #[test]
    fn executor_with_defaults_succeeds() {
        let result = Executor::with_defaults();
        assert!(result.is_ok(), "should create executor with defaults");
    }

    #[test]
    fn mutant_kill_default_provider_returns_set_value() {
        // Mutant kill: executor.rs:126 — default_provider() replaced with None
        let executor = Executor::with_defaults()
            .unwrap_or_else(|_| std::process::abort())
            .with_default_provider(Provider::OpenRouter);
        let provider = executor.default_provider();
        assert!(
            provider.is_some(),
            "default_provider() must return Some after with_default_provider()"
        );
        assert!(
            matches!(provider, Some(Provider::OpenRouter)),
            "default_provider() must return the provider that was set"
        );
    }

    #[test]
    fn mutant_kill_is_remote_default_false() {
        // Mutant kill: executor.rs:141 — is_remote() replaced with true
        let executor = Executor::with_defaults()
            .unwrap_or_else(|_| std::process::abort());
        assert!(
            !executor.is_remote(),
            "is_remote() must be false by default, not true"
        );
    }

    #[test]
    fn mutant_kill_is_remote_true_after_with_remote() {
        // Mutant kill: executor.rs:141 — is_remote() replaced with false
        let executor = Executor::with_defaults()
            .unwrap_or_else(|_| std::process::abort())
            .with_remote();
        assert!(
            executor.is_remote(),
            "is_remote() must be true after with_remote()"
        );
    }

    #[test]
    fn executor_with_dry_run() {
        let executor = Executor::with_defaults()
            .unwrap_or_else(|_| {
                Executor::new(&ClientConfig::default())
                    .unwrap_or_else(|_| std::process::abort())
            })
            .with_dry_run(PathBuf::from("/tmp/test-dry-run"));

        assert!(
            executor.dry_run_config().is_some(),
            "dry run should be enabled"
        );
    }
}
