//! Typed LLM model and provider selection.

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
    /// Get the model ID string for the given provider.
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
            Provider::Anthropic | Provider::Custom(_) | Provider::Vertex => String::from(base),
            Provider::OpenRouter => format!("anthropic/{base}"),
            Provider::Bedrock => format!("anthropic.{base}-v1"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "Vertex Sonnet model ID"
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
}
