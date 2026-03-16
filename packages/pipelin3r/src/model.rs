//! Typed LLM model and provider selection with TOML-based configuration.

use std::collections::BTreeMap;
use std::path::Path;

/// Embedded default model configuration, compiled from `models.toml`.
const DEFAULT_MODELS_TOML: &str = include_str!("../models.toml");

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

/// Loaded model ID configuration, grouped by provider.
///
/// Holds a mapping of `provider_key -> model_key -> model_id_string`.
/// Use [`ModelConfig::resolve`] to look up the model ID for a given
/// model/provider pair, falling back to hardcoded defaults when the
/// config does not contain the entry.
#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
    providers: BTreeMap<String, BTreeMap<String, String>>,
}

impl ModelConfig {
    /// Load model configuration from a TOML string.
    ///
    /// # Errors
    /// Returns an error if the TOML string cannot be parsed.
    pub fn from_toml(toml_str: &str) -> Result<Self, crate::error::PipelineError> {
        let providers: BTreeMap<String, BTreeMap<String, String>> = toml::from_str(toml_str)
            .map_err(|e| crate::error::PipelineError::Config(format!("failed to parse model TOML: {e}")))?;
        Ok(Self { providers })
    }

    /// Load model configuration from a TOML file path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &Path) -> Result<Self, crate::error::PipelineError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::PipelineError::Config(format!("failed to read model config {}: {e}", path.display()))
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
mod tests {
    use super::*;

    // --- Tool enum tests ---

    #[test]
    fn tool_as_str_known_variants() {
        assert_eq!(Tool::Read.as_str(), "Read", "Read tool name");
        assert_eq!(Tool::Write.as_str(), "Write", "Write tool name");
        assert_eq!(Tool::Grep.as_str(), "Grep", "Grep tool name");
        assert_eq!(Tool::Glob.as_str(), "Glob", "Glob tool name");
        assert_eq!(Tool::WebSearch.as_str(), "WebSearch", "WebSearch tool name");
        assert_eq!(Tool::WebFetch.as_str(), "WebFetch", "WebFetch tool name");
    }

    #[test]
    fn tool_as_str_custom() {
        let tool = Tool::Custom(String::from("MyCustomTool"));
        assert_eq!(
            tool.as_str(),
            "MyCustomTool",
            "Custom tool should pass through"
        );
    }

    #[test]
    fn regression_tool_enum_returns_strings_not_indices() {
        // Regression: Tool variants returned numeric indices or debug strings
        // instead of the expected CLI tool name strings.
        assert_eq!(
            Tool::Read.as_str(),
            "Read",
            "Tool::Read must return \"Read\", not a number or debug repr"
        );
        assert_eq!(
            Tool::Custom(String::from("Mcp")).as_str(),
            "Mcp",
            "Tool::Custom must return the inner string as-is"
        );
        // Verify all known variants produce non-empty proper strings.
        let known = [
            Tool::Read, Tool::Write, Tool::Grep,
            Tool::Glob, Tool::WebSearch, Tool::WebFetch,
        ];
        for tool in &known {
            let s = tool.as_str();
            assert!(!s.is_empty(), "tool string must not be empty");
            assert!(
                s.chars().next().is_some_and(char::is_uppercase),
                "tool string must start with uppercase: {s}"
            );
        }
    }

    #[test]
    fn tool_display() {
        assert_eq!(format!("{}", Tool::Read), "Read", "Display for Read");
        assert_eq!(
            format!("{}", Tool::Custom(String::from("X"))),
            "X",
            "Display for Custom"
        );
    }

    // --- Model tests ---

    #[test]
    fn anthropic_opus() {
        assert_eq!(
            Model::Opus4_6.id(&Provider::Anthropic),
            "claude-opus-4-6",
            "Anthropic Opus model ID"
        );
    }

    #[test]
    fn anthropic_sonnet() {
        assert_eq!(
            Model::Sonnet4_6.id(&Provider::Anthropic),
            "claude-sonnet-4-6",
            "Anthropic Sonnet model ID"
        );
    }

    #[test]
    fn anthropic_haiku() {
        assert_eq!(
            Model::Haiku4_5.id(&Provider::Anthropic),
            "claude-haiku-4-5",
            "Anthropic Haiku model ID"
        );
    }

    #[test]
    fn openrouter_opus() {
        assert_eq!(
            Model::Opus4_6.id(&Provider::OpenRouter),
            "anthropic/claude-opus-4-6",
            "OpenRouter Opus model ID"
        );
    }

    #[test]
    fn openrouter_sonnet() {
        assert_eq!(
            Model::Sonnet4_6.id(&Provider::OpenRouter),
            "anthropic/claude-sonnet-4-6",
            "OpenRouter Sonnet model ID"
        );
    }

    #[test]
    fn bedrock_opus() {
        assert_eq!(
            Model::Opus4_6.id(&Provider::Bedrock),
            "anthropic.claude-opus-4-6-v1",
            "Bedrock Opus model ID"
        );
    }

    #[test]
    fn bedrock_haiku() {
        assert_eq!(
            Model::Haiku4_5.id(&Provider::Bedrock),
            "anthropic.claude-haiku-4-5-v1",
            "Bedrock Haiku model ID"
        );
    }

    #[test]
    fn vertex_sonnet() {
        assert_eq!(
            Model::Sonnet4_6.id(&Provider::Vertex),
            "claude-sonnet-4-6",
            "Vertex Sonnet model ID (hardcoded fallback)"
        );
    }

    #[test]
    fn custom_model_passthrough() {
        let model = Model::Custom(String::from("my-fine-tuned-model"));
        assert_eq!(
            model.id(&Provider::Anthropic),
            "my-fine-tuned-model",
            "Custom model should pass through for Anthropic"
        );
        assert_eq!(
            model.id(&Provider::OpenRouter),
            "my-fine-tuned-model",
            "Custom model should pass through for OpenRouter"
        );
        assert_eq!(
            model.id(&Provider::Bedrock),
            "my-fine-tuned-model",
            "Custom model should pass through for Bedrock"
        );
    }

    #[test]
    fn custom_provider_uses_base_ids() {
        let provider = Provider::Custom(String::from("my-provider"));
        assert_eq!(
            Model::Opus4_6.id(&provider),
            "claude-opus-4-6",
            "Custom provider should use base model IDs"
        );
    }

    #[test]
    fn default_provider_is_anthropic() {
        let provider = Provider::default();
        assert_eq!(
            Model::Opus4_6.id(&provider),
            "claude-opus-4-6",
            "Default provider should behave like Anthropic"
        );
    }

    // --- ModelConfig tests ---

    #[test]
    fn config_from_toml_roundtrip() {
        let toml_str = r#"
[anthropic]
opus_4_6 = "claude-opus-4-6"
sonnet_4_6 = "claude-sonnet-4-6"

[openrouter]
opus_4_6 = "anthropic/claude-opus-4-6"
"#;
        let config = ModelConfig::from_toml(toml_str);
        assert!(config.is_ok(), "should parse valid TOML");
        let config = config.unwrap_or_default();
        assert_eq!(
            config
                .providers
                .get("anthropic")
                .and_then(|m| m.get("opus_4_6")),
            Some(&String::from("claude-opus-4-6")),
            "should contain anthropic opus entry"
        );
    }

    #[test]
    fn config_resolve_with_loaded_config() {
        let toml_str = r#"
[vertex]
opus_4_6 = "claude-opus-4-6@20250514"
sonnet_4_6 = "claude-sonnet-4-6@20250514"
haiku_4_5 = "claude-haiku-4-5@20251001"
"#;
        let config = ModelConfig::from_toml(toml_str).unwrap_or_default();
        assert_eq!(
            config.resolve(&Model::Opus4_6, &Provider::Vertex),
            "claude-opus-4-6@20250514",
            "should resolve vertex opus from config"
        );
        assert_eq!(
            config.resolve(&Model::Haiku4_5, &Provider::Vertex),
            "claude-haiku-4-5@20251001",
            "should resolve vertex haiku from config"
        );
    }

    #[test]
    fn config_resolve_fallback_for_missing_entry() {
        let toml_str = r#"
[anthropic]
opus_4_6 = "claude-opus-4-6"
"#;
        let config = ModelConfig::from_toml(toml_str).unwrap_or_default();
        // haiku_4_5 is not in config — should fall back to hardcoded.
        assert_eq!(
            config.resolve(&Model::Haiku4_5, &Provider::Anthropic),
            "claude-haiku-4-5",
            "should fall back to hardcoded for missing model key"
        );
        // openrouter is not in config at all — should fall back to hardcoded.
        assert_eq!(
            config.resolve(&Model::Opus4_6, &Provider::OpenRouter),
            "anthropic/claude-opus-4-6",
            "should fall back to hardcoded for missing provider"
        );
    }

    #[test]
    fn config_default_loads_embedded_toml() {
        let config = ModelConfig::default_config();
        assert_eq!(
            config.resolve(&Model::Opus4_6, &Provider::Anthropic),
            "claude-opus-4-6",
            "default config should resolve anthropic opus"
        );
        assert_eq!(
            config.resolve(&Model::Sonnet4_6, &Provider::OpenRouter),
            "anthropic/claude-sonnet-4-6",
            "default config should resolve openrouter sonnet"
        );
        assert_eq!(
            config.resolve(&Model::Haiku4_5, &Provider::Bedrock),
            "anthropic.claude-haiku-4-5-v1",
            "default config should resolve bedrock haiku"
        );
        assert_eq!(
            config.resolve(&Model::Opus4_6, &Provider::Vertex),
            "claude-opus-4-6@20250514",
            "default config should resolve vertex opus with date suffix"
        );
    }

    #[test]
    fn config_custom_model_bypasses_config() {
        let config = ModelConfig::default_config();
        let model = Model::Custom(String::from("my-custom-model"));
        assert_eq!(
            config.resolve(&model, &Provider::Anthropic),
            "my-custom-model",
            "custom model should bypass config lookup"
        );
        assert_eq!(
            config.resolve(&model, &Provider::OpenRouter),
            "my-custom-model",
            "custom model should bypass config for all providers"
        );
    }

    #[test]
    fn config_custom_provider_bypasses_config() {
        let config = ModelConfig::default_config();
        let provider = Provider::Custom(String::from("my-provider"));
        assert_eq!(
            config.resolve(&Model::Opus4_6, &provider),
            "claude-opus-4-6",
            "custom provider should bypass config and use hardcoded base ID"
        );
    }

    #[test]
    fn config_from_toml_invalid() {
        let result = ModelConfig::from_toml("this is not [valid toml");
        assert!(result.is_err(), "should fail on invalid TOML");
    }

    #[test]
    fn config_from_file_nonexistent() {
        let result = ModelConfig::from_file(Path::new("/nonexistent/models.toml"));
        assert!(result.is_err(), "should fail on nonexistent file");
    }

    #[test]
    fn config_empty_toml_falls_back() {
        let config = ModelConfig::from_toml("").unwrap_or_default();
        assert_eq!(
            config.resolve(&Model::Opus4_6, &Provider::Anthropic),
            "claude-opus-4-6",
            "empty config should fall back to hardcoded"
        );
    }

    #[test]
    fn config_key_model_variants() {
        assert_eq!(
            Model::Opus4_6.config_key(),
            Some("opus_4_6"),
            "Opus4_6 config key"
        );
        assert_eq!(
            Model::Sonnet4_6.config_key(),
            Some("sonnet_4_6"),
            "Sonnet4_6 config key"
        );
        assert_eq!(
            Model::Haiku4_5.config_key(),
            Some("haiku_4_5"),
            "Haiku4_5 config key"
        );
        assert_eq!(
            Model::Custom(String::from("x")).config_key(),
            None,
            "Custom model has no config key"
        );
    }

    #[test]
    fn config_key_provider_variants() {
        assert_eq!(
            Provider::Anthropic.config_key(),
            Some("anthropic"),
            "Anthropic config key"
        );
        assert_eq!(
            Provider::OpenRouter.config_key(),
            Some("openrouter"),
            "OpenRouter config key"
        );
        assert_eq!(
            Provider::Bedrock.config_key(),
            Some("bedrock"),
            "Bedrock config key"
        );
        assert_eq!(
            Provider::Vertex.config_key(),
            Some("vertex"),
            "Vertex config key"
        );
        assert_eq!(
            Provider::Custom(String::from("x")).config_key(),
            None,
            "Custom provider has no config key"
        );
    }
}
