//! HTTP client functions for the ``OpenRouter`` image generation API.

use base64::Engine as _;

use crate::error::PipelineError;

use super::types::{
    AspectRatio, ChatCompletionRequest, ChatCompletionResponse, ContentPart, GeneratedImage,
    GenerationCostResponse, ImageConfig, ImageModel, ImageUrlValue, Message, RefImage,
};

/// ``OpenRouter`` chat completions endpoint.
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// ``OpenRouter`` generation cost lookup endpoint prefix.
const OPENROUTER_GENERATION_URL: &str = "https://openrouter.ai/api/v1/generation?id=";

/// Build the JSON request body for an image generation call.
pub fn generate_image_request(
    model: &ImageModel,
    prompt: &str,
    reference_images: &[RefImage],
    aspect_ratio: &AspectRatio,
) -> ChatCompletionRequest {
    let mut content_parts: Vec<ContentPart> = Vec::new();

    // Add reference images first (order matters for some models).
    for img in reference_images {
        content_parts.push(ContentPart::ImageUrl {
            image_url: ImageUrlValue {
                url: img.as_data_url(),
            },
        });
    }

    // Add the text prompt.
    content_parts.push(ContentPart::Text {
        text: String::from(prompt),
    });

    ChatCompletionRequest {
        model: String::from(model.as_openrouter_id()),
        modalities: vec![String::from("image"), String::from("text")],
        messages: vec![Message {
            role: String::from("user"),
            content: content_parts,
        }],
        image_config: Some(ImageConfig {
            aspect_ratio: String::from(aspect_ratio.as_str()),
        }),
    }
}

/// Parse a chat completion response, extracting the generated image.
///
/// Decodes the base64 image data from the first image in the first choice.
///
/// # Errors
/// Returns an error if no image is found or base64 decoding fails.
pub fn parse_image_response(
    response: &ChatCompletionResponse,
) -> Result<GeneratedImage, PipelineError> {
    // Navigate to the first image URL.
    let image_url = response
        .choices
        .first()
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.images.first())
        .and_then(|i| i.image_url.as_ref())
        .map(|u| u.url.as_str())
        .ok_or_else(|| PipelineError::ImageGenFailed {
            message: String::from("no image found in API response"),
        })?;

    // Parse the data URL: "data:{mime};base64,{data}"
    let (mime, base64_data) = parse_data_url(image_url)?;

    // Decode base64.
    let data = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| PipelineError::ImageGenFailed {
            message: format!("failed to decode base64 image data: {e}"),
        })?;

    // Extract cost from usage.
    let cost = response.usage.as_ref().and_then(|u| u.cost);
    let generation_id = response.id.clone();

    Ok(GeneratedImage {
        data,
        mime,
        cost,
        generation_id,
    })
}

/// Parse a `data:` URL into (`mime_type`, `base64_data`).
#[allow(
    clippy::type_complexity,
    reason = "return type is a simple tuple wrapped in Result"
)]
fn parse_data_url(url: &str) -> Result<(String, &str), PipelineError> {
    // Expected format: "data:image/png;base64,iVBOR..."
    let rest = url
        .strip_prefix("data:")
        .ok_or_else(|| PipelineError::ImageGenFailed {
            message: String::from("image URL does not start with 'data:'"),
        })?;

    let semi_pos = rest
        .find(';')
        .ok_or_else(|| PipelineError::ImageGenFailed {
            message: String::from("malformed data URL: missing ';'"),
        })?;

    let mime = rest
        .get(..semi_pos)
        .ok_or_else(|| PipelineError::ImageGenFailed {
            message: String::from("malformed data URL: failed to extract MIME type"),
        })?;

    let after_semi =
        rest.get(semi_pos.saturating_add(1)..)
            .ok_or_else(|| PipelineError::ImageGenFailed {
                message: String::from("malformed data URL: nothing after ';'"),
            })?;

    let base64_data =
        after_semi
            .strip_prefix("base64,")
            .ok_or_else(|| PipelineError::ImageGenFailed {
                message: String::from("malformed data URL: missing 'base64,' prefix"),
            })?;

    Ok((String::from(mime), base64_data))
}

/// Send the image generation request to `OpenRouter`.
///
/// # Errors
/// Returns an error if the HTTP request fails or the response indicates an error.
#[allow(
    clippy::disallowed_methods,
    reason = "reqwest .json() is the standard way to send/receive JSON"
)]
pub async fn send_image_request(
    client: &reqwest::Client,
    api_key: &str,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, PipelineError> {
    let response = client
        .post(OPENROUTER_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(request)
        .send()
        .await
        .map_err(|e| PipelineError::ImageGenFailed {
            message: format!("HTTP request failed: {e}"),
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default(); // allow: best-effort body read for error message
        return Err(PipelineError::ImageGenFailed {
            message: format!("`OpenRouter` returned {status}: {body}"),
        });
    }

    let parsed: ChatCompletionResponse =
        response
            .json()
            .await
            .map_err(|e| PipelineError::ImageGenFailed {
                message: format!("failed to parse response JSON: {e}"),
            })?;

    Ok(parsed)
}

/// Follow-up cost lookup via the generation endpoint.
///
/// Some models report cost as 0 in the initial response. This function
/// queries the dedicated generation endpoint for the actual cost.
///
/// # Errors
/// Returns `None` on any failure (best-effort).
#[allow(
    clippy::disallowed_methods,
    reason = "reqwest .json() is the standard way to receive JSON"
)]
pub async fn fetch_cost(
    client: &reqwest::Client,
    api_key: &str,
    generation_id: &str,
) -> Option<f64> {
    let url = format!("{OPENROUTER_GENERATION_URL}{generation_id}");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let parsed: GenerationCostResponse = response.json().await.ok()?;
    parsed.data.and_then(|d| d.total_cost)
}
