//! Type definitions for image generation via the `OpenRouter` API.

use serde::{Deserialize, Serialize};

// ── Public enums ──

/// Aspect ratio for generated images.
///
/// Maps to the `aspect_ratio` field in the `OpenRouter` `image_config` object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AspectRatio {
    /// 1:1 square.
    #[default]
    Square,
    /// 16:9 landscape (widescreen).
    Landscape16x9,
    /// 9:16 portrait.
    Portrait9x16,
    /// 3:2 landscape.
    Landscape3x2,
    /// 2:3 portrait.
    Portrait2x3,
    /// 4:3 landscape.
    Landscape4x3,
    /// 3:4 portrait.
    Portrait3x4,
    /// 4:5 portrait.
    Portrait4x5,
    /// 5:4 landscape.
    Landscape5x4,
    /// 21:9 ultrawide.
    Ultrawide,
    /// Custom aspect ratio string (e.g. `"1:4"`).
    Custom(String),
}

impl AspectRatio {
    /// Return the API string representation (e.g. `"16:9"`).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Square => "1:1",
            Self::Landscape16x9 => "16:9",
            Self::Portrait9x16 => "9:16",
            Self::Landscape3x2 => "3:2",
            Self::Portrait2x3 => "2:3",
            Self::Landscape4x3 => "4:3",
            Self::Portrait3x4 => "3:4",
            Self::Portrait4x5 => "4:5",
            Self::Landscape5x4 => "5:4",
            Self::Ultrawide => "21:9",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// Known image generation models available via `OpenRouter`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ImageModel {
    /// Google Gemini 3.1 Flash Image Preview.
    Gemini3_1Flash,
    /// Google Gemini 2.5 Flash Image Preview.
    #[default]
    Gemini2_5Flash,
    /// Custom model identifier string.
    Custom(String),
}

impl ImageModel {
    /// Return the `OpenRouter` model ID string.
    pub fn as_openrouter_id(&self) -> &str {
        match self {
            Self::Gemini3_1Flash => "google/gemini-3.1-flash-image-preview",
            Self::Gemini2_5Flash => "google/gemini-2.5-flash-preview-image",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// Role of a reference image in the generation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefImageRole {
    /// Style reference (the model should match the visual style).
    Style,
    /// Character sheet / reference sheet.
    CharSheet,
    /// Generic input image.
    Input,
}

/// A reference image attached to an image generation request.
#[derive(Debug, Clone)]
pub struct RefImage {
    /// MIME type (e.g. `"image/png"`).
    pub mime: String,
    /// Base64-encoded image data.
    pub data: String,
    /// Semantic role of this reference image.
    pub role: RefImageRole,
}

impl RefImage {
    /// Create a new reference image.
    pub fn new(mime: impl Into<String>, data: impl Into<String>, role: RefImageRole) -> Self {
        Self {
            mime: mime.into(),
            data: data.into(),
            role,
        }
    }

    /// Load a reference image from a file path.
    ///
    /// Reads the file, base64-encodes the contents, and infers the MIME type
    /// from the file extension.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the extension is unrecognised.
    #[allow(
        clippy::disallowed_methods,
        reason = "reading reference image file requires direct filesystem access"
    )]
    pub fn from_file(
        path: &std::path::Path,
        role: RefImageRole,
    ) -> Result<Self, crate::PipelineError> {
        use base64::Engine as _;

        let data = crate::fs::read(path).map_err(|e| {
            crate::PipelineError::Transport(format!(
                "failed to read reference image {}: {e}",
                path.display()
            ))
        })?;

        let mime = mime_from_extension(path)?;

        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

        Ok(Self {
            mime,
            data: encoded,
            role,
        })
    }

    /// Build the `data:` URL for embedding in an API request content part.
    pub(crate) fn as_data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data)
    }
}

/// Infer MIME type from a file extension.
fn mime_from_extension(path: &std::path::Path) -> Result<String, crate::PipelineError> {
    let ext = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");
    let mime = match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        other => {
            return Err(crate::PipelineError::Config(format!(
                "unrecognised image extension '{other}' for {}",
                path.display()
            )));
        }
    };
    Ok(String::from(mime))
}

// ── Serde types for the `OpenRouter` chat-completions API ──

/// Request body sent to the `OpenRouter` chat completions endpoint.
#[derive(Debug, Serialize)]
pub(crate) struct ChatCompletionRequest {
    /// Model identifier.
    pub model: String,
    /// Output modalities (must include `"image"` for image generation).
    pub modalities: Vec<String>,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Image configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<ImageConfig>,
}

/// A single message in the chat completions request.
#[derive(Debug, Serialize)]
pub(crate) struct Message {
    /// Role (always `"user"` for image generation).
    pub role: String,
    /// Content parts (text and/or image references).
    pub content: Vec<ContentPart>,
}

/// A content part within a message.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(crate) enum ContentPart {
    /// Text content.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Image URL reference (for reference images).
    #[serde(rename = "image_url")]
    ImageUrl {
        /// The image URL object.
        image_url: ImageUrlValue,
    },
}

/// Image URL value within a content part.
#[derive(Debug, Serialize)]
pub(crate) struct ImageUrlValue {
    /// Data URL or HTTP URL pointing to the image.
    pub url: String,
}

/// Image configuration in the request body.
#[derive(Debug, Serialize)]
pub(crate) struct ImageConfig {
    /// Aspect ratio string (e.g. `"16:9"`).
    pub aspect_ratio: String,
}

// ── Response types ──

/// Top-level response from the `OpenRouter` chat completions endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionResponse {
    /// Generation ID (used for cost lookup).
    pub id: Option<String>,
    /// Response choices.
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// Usage/cost information.
    pub usage: Option<Usage>,
}

/// A single choice in the response.
#[derive(Debug, Deserialize)]
pub(crate) struct Choice {
    /// The assistant's message.
    pub message: Option<ResponseMessage>,
}

/// The assistant message within a choice.
#[derive(Debug, Deserialize)]
pub(crate) struct ResponseMessage {
    /// Generated images.
    #[serde(default)]
    pub images: Vec<ResponseImage>,
}

/// A generated image in the response.
#[derive(Debug, Deserialize)]
pub(crate) struct ResponseImage {
    /// Image URL container.
    pub image_url: Option<ResponseImageUrl>,
}

/// Image URL value in the response.
#[derive(Debug, Deserialize)]
pub(crate) struct ResponseImageUrl {
    /// Data URL containing the base64-encoded image.
    pub url: String,
}

/// Usage information from the response.
#[derive(Debug, Deserialize)]
pub(crate) struct Usage {
    /// Cost in USD.
    pub cost: Option<f64>,
}

/// Response from the `OpenRouter` generation cost lookup endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct GenerationCostResponse {
    /// Generation data.
    pub data: Option<GenerationCostData>,
}

/// Generation cost data.
#[derive(Debug, Deserialize)]
pub(crate) struct GenerationCostData {
    /// Total cost in USD.
    pub total_cost: Option<f64>,
}

/// Parsed result from a successful image generation HTTP call.
#[derive(Debug)]
pub(crate) struct GeneratedImage {
    /// Raw image bytes (decoded from base64).
    pub data: Vec<u8>,
    /// Detected MIME type from the data URL.
    pub mime: String,
    /// API cost in USD.
    pub cost: Option<f64>,
    /// Generation ID for follow-up cost lookup.
    pub generation_id: Option<String>,
}
