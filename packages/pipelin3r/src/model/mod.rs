//! Typed LLM model and provider selection with TOML-based configuration.

use std::collections::BTreeMap;
use std::path::Path;

/// Embedded default model configuration, compiled from `models.toml`.
const DEFAULT_MODELS_TOML: &str = include_str!("../../models.toml");

/// Tool available to an agent during invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool {
    /// File reading tool.
    Read,
    /// File writing tool.
    Write,
    /// Content search (grep) tool.
    Grep,
    /// File pattern matching (glob) tool.
    Glob,
    /// Web search tool.
    WebSearch,
    /// Web page fetch tool.
    WebFetch,
    /// Custom tool name (passed through as-is).
    Custom(String),
}

impl Tool {
    /// Get the tool name string used in the CLI `--allowedTools` flag.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Read => "Read",
            Self::Write => "Write",
            Self::Grep => "Grep",
            Self::Glob => "Glob",
            Self::WebSearch => "WebSearch",
            Self::WebFetch => "WebFetch",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// LLM model selection.
#[derive(Debug, Clone)]
pub enum Model {
    /// Claude Opus 4.6
    Opus4_6,
    /// Claude Sonnet 4.6
    Sonnet4_6,
    /// Claude Haiku 4.5
    Haiku4_5,
    /// Custom model identifier (passed through as-is).
    Custom(String),
}

/// LLM provider.
#[derive(Debug, Clone, Default)]
pub enum Provider {
    /// Anthropic direct API.
    #[default]
    Anthropic,
    /// `OpenRouter` proxy.
    OpenRouter,
    /// AWS Bedrock.
    Bedrock,
    /// Google Vertex AI.
    Vertex,
    /// Custom provider (model IDs passed through as-is).
    Custom(String),
}

impl Model {
    /// Get the model ID string for the given provider using hardcoded defaults.
    ///
    /// Known models are mapped to provider-specific identifiers.
    /// `Custom` variants are passed through unchanged regardless of provider.
    pub fn id(&self, provider: &Provider) -> String {
        let base = match self {
            Self::Opus4_6 => "claude-opus-4-6",
            Self::Sonnet4_6 => "claude-sonnet-4-6",
            Self::Haiku4_5 => "claude-haiku-4-5",
            Self::Custom(id) => return id.clone(),
        };
        match provider {
            Provider::OpenRouter => format!("anthropic/{base}"),
            Provider::Bedrock => format!("anthropic.{base}-v1"),
            Provider::Anthropic | Provider::Custom(_) | Provider::Vertex => String::from(base),
        }
    }

    /// Get the TOML configuration key for this model variant.
    ///
    /// Returns `None` for `Custom` models (they bypass configuration).
    const fn config_key(&self) -> Option<&str> {
        match self {
            Self::Opus4_6 => Some("opus_4_6"),
            Self::Sonnet4_6 => Some("sonnet_4_6"),
            Self::Haiku4_5 => Some("haiku_4_5"),
            Self::Custom(_) => None,
        }
    }
}

impl Provider {
    /// Get the TOML configuration key for this provider variant.
    ///
    /// Returns `None` for `Custom` providers (they bypass configuration).
    const fn config_key(&self) -> Option<&str> {
        match self {
            Self::Anthropic => Some("anthropic"),
            Self::OpenRouter => Some("openrouter"),
            Self::Bedrock => Some("bedrock"),
            Self::Vertex => Some("vertex"),
            Self::Custom(_) => None,
        }
    }
}

/// Type alias for the provider-to-model ID mapping.
type ProviderModelMap = BTreeMap<String, BTreeMap<String, String>>;

/// Loaded model ID configuration, grouped by provider.
///
/// Holds a mapping of `provider_key -> model_key -> model_id_string`.
/// Use [`ModelConfig::resolve`] to look up the model ID for a given
/// model/provider pair, falling back to hardcoded defaults when the
/// config does not contain the entry.
#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
    providers: ProviderModelMap,
}

impl ModelConfig {
    /// Load model configuration from a TOML string.
    ///
    /// # Errors
    /// Returns an error if the TOML string cannot be parsed.
    #[allow(
        clippy::disallowed_methods,
        reason = "model config: parsing embedded TOML configuration"
    )]
    pub fn from_toml(toml_str: &str) -> Result<Self, crate::error::PipelineError> {
        let providers: ProviderModelMap = toml::from_str(toml_str).map_err(|e| {
            crate::error::PipelineError::Config(format!("failed to parse model TOML: {e}"))
        })?;
        Ok(Self { providers })
    }

    /// Load model configuration from a TOML file path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &Path) -> Result<Self, crate::error::PipelineError> {
        let content = crate::fs::read_to_string(path).map_err(|e| {
            crate::error::PipelineError::Config(format!(
                "failed to read model config {}: {e}",
                path.display()
            ))
        })?;
        Self::from_toml(&content)
    }

    /// Load the built-in default configuration (embedded at compile time).
    ///
    /// # Panics
    /// This function does not panic. The embedded TOML is validated at
    /// development time, so parsing is expected to succeed. If parsing
    /// fails, an empty config is returned and all lookups fall through
    /// to hardcoded defaults.
    pub fn default_config() -> Self {
        Self::from_toml(DEFAULT_MODELS_TOML).unwrap_or_default()
    }

    /// Resolve a model ID for the given model and provider.
    ///
    /// Looks up the model ID in the loaded configuration first.
    /// Falls back to the hardcoded [`Model::id`] method if:
    /// - The model is `Custom` (always passed through as-is)
    /// - The provider is `Custom` (always uses hardcoded logic)
    /// - The configuration does not contain the provider or model key
    pub fn resolve(&self, model: &Model, provider: &Provider) -> String {
        let Some(provider_key) = provider.config_key() else {
            return model.id(provider);
        };
        let Some(model_key) = model.config_key() else {
            return model.id(provider);
        };

        self.providers
            .get(provider_key)
            .and_then(|models| models.get(model_key))
            .cloned()
            .unwrap_or_else(|| model.id(provider))
    }
}

#[cfg(test)]
mod tests;
