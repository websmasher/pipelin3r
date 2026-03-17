#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct client/env access"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "test assertions: index panic is acceptable"
)]

//! Unit tests for image generation types and client logic.

use super::client::{generate_image_request, parse_image_response};
use super::types::{
    AspectRatio, ChatCompletionResponse, Choice, ImageModel, RefImage, RefImageRole, ResponseImage,
    ResponseImageUrl, ResponseMessage, Usage,
};
use super::{ImageGenConfig, ImageGenHttpConfig, ImageGenResult};
use std::path::PathBuf;

// ── AspectRatio tests ──

#[test]
fn aspect_ratio_as_str_covers_all_variants() {
    assert_eq!(AspectRatio::Square.as_str(), "1:1");
    assert_eq!(AspectRatio::Landscape16x9.as_str(), "16:9");
    assert_eq!(AspectRatio::Portrait9x16.as_str(), "9:16");
    assert_eq!(AspectRatio::Landscape3x2.as_str(), "3:2");
    assert_eq!(AspectRatio::Portrait2x3.as_str(), "2:3");
    assert_eq!(AspectRatio::Landscape4x3.as_str(), "4:3");
    assert_eq!(AspectRatio::Portrait3x4.as_str(), "3:4");
    assert_eq!(AspectRatio::Portrait4x5.as_str(), "4:5");
    assert_eq!(AspectRatio::Landscape5x4.as_str(), "5:4");
    assert_eq!(AspectRatio::Ultrawide.as_str(), "21:9");
    assert_eq!(AspectRatio::Custom(String::from("1:4")).as_str(), "1:4");
}

#[test]
fn aspect_ratio_default_is_square() {
    assert_eq!(AspectRatio::default(), AspectRatio::Square);
}

// ── ImageModel tests ──

#[test]
fn image_model_openrouter_ids() {
    assert_eq!(
        ImageModel::Gemini3_1Flash.as_openrouter_id(),
        "google/gemini-3.1-flash-image-preview"
    );
    assert_eq!(
        ImageModel::Gemini2_5Flash.as_openrouter_id(),
        "google/gemini-2.5-flash-preview-image"
    );
    assert_eq!(
        ImageModel::Custom(String::from("my/model")).as_openrouter_id(),
        "my/model"
    );
}

#[test]
fn image_model_default_is_gemini_2_5_flash() {
    assert_eq!(ImageModel::default(), ImageModel::Gemini2_5Flash);
}

// ── RefImage tests ──

#[test]
fn ref_image_data_url_format() {
    let img = RefImage::new("image/png", "abc123", RefImageRole::Style);
    assert_eq!(img.as_data_url(), "data:image/png;base64,abc123");
}

// ── Request building tests ──

#[test]
fn generate_image_request_no_refs() {
    let req = generate_image_request(
        &ImageModel::Gemini2_5Flash,
        "A sunset",
        &[],
        &AspectRatio::Landscape16x9,
    );

    assert_eq!(req.model, "google/gemini-2.5-flash-preview-image");
    assert_eq!(req.modalities, vec!["image", "text"]);
    assert_eq!(req.messages.len(), 1);
    assert_eq!(req.messages[0].role, "user");
    // Only text part, no image refs.
    assert_eq!(req.messages[0].content.len(), 1);

    let config = req.image_config.as_ref().map(|c| c.aspect_ratio.as_str());
    assert_eq!(config, Some("16:9"));
}

#[test]
fn generate_image_request_with_refs() {
    let refs = vec![
        RefImage::new("image/png", "ref1data", RefImageRole::Style),
        RefImage::new("image/jpeg", "ref2data", RefImageRole::CharSheet),
    ];

    let req = generate_image_request(
        &ImageModel::Gemini3_1Flash,
        "Draw a cat",
        &refs,
        &AspectRatio::Square,
    );

    // 2 image parts + 1 text part = 3 content parts.
    assert_eq!(req.messages[0].content.len(), 3);
}

// ── Response parsing tests ──

#[test]
fn parse_image_response_success() {
    // Build a minimal valid base64 PNG header (1x1 transparent pixel).
    use base64::Engine as _;
    let tiny_png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
    ];
    let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_png);

    let response = ChatCompletionResponse {
        id: Some(String::from("gen-123")),
        choices: vec![Choice {
            message: Some(ResponseMessage {
                images: vec![ResponseImage {
                    image_url: Some(ResponseImageUrl {
                        url: format!("data:image/png;base64,{b64}"),
                    }),
                }],
            }),
        }],
        usage: Some(Usage { cost: Some(0.05) }),
    };

    let result = parse_image_response(&response);
    assert!(result.is_ok());

    let generated = result.unwrap();
    assert_eq!(generated.data, tiny_png);
    assert_eq!(generated.mime, "image/png");
    assert_eq!(generated.cost, Some(0.05));
    assert_eq!(generated.generation_id.as_deref(), Some("gen-123"));
}

#[test]
fn parse_image_response_no_image_returns_error() {
    let response = ChatCompletionResponse {
        id: None,
        choices: vec![],
        usage: None,
    };

    let result = parse_image_response(&response);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("no image found"), "unexpected error: {msg}");
}

#[test]
fn parse_image_response_bad_data_url_returns_error() {
    let response = ChatCompletionResponse {
        id: None,
        choices: vec![Choice {
            message: Some(ResponseMessage {
                images: vec![ResponseImage {
                    image_url: Some(ResponseImageUrl {
                        url: String::from("https://example.com/image.png"),
                    }),
                }],
            }),
        }],
        usage: None,
    };

    let result = parse_image_response(&response);
    assert!(result.is_err());
}

// ── ImageGenConfig tests ──

#[test]
fn image_gen_config_defaults() {
    let config = ImageGenConfig::new("test prompt", PathBuf::from("/tmp"));
    assert_eq!(config.prompt, "test prompt");
    assert_eq!(config.model, ImageModel::Gemini2_5Flash);
    assert_eq!(config.aspect_ratio, AspectRatio::Square);
    assert_eq!(config.output_filename, "generated.png");
    assert!(config.reference_images.is_empty());
}

// ── ImageGenResult tests ──

#[test]
fn image_gen_result_require_success_ok() {
    let result = ImageGenResult {
        success: true,
        output_files: vec![PathBuf::from("/tmp/out.png")],
        cost: Some(0.05),
        output_mime: Some(String::from("image/png")),
    };
    assert!(result.require_success().is_ok());
}

#[test]
fn image_gen_result_require_success_err() {
    let result = ImageGenResult {
        success: false,
        output_files: vec![],
        cost: None,
        output_mime: None,
    };
    assert!(result.require_success().is_err());
}

// ── ImageGenHttpConfig tests ──

#[test]
fn http_config_debug_redacts_key() {
    let config = ImageGenHttpConfig::new("secret-key-123");
    let debug = format!("{config:?}");
    assert!(
        !debug.contains("secret-key-123"),
        "API key should be redacted in Debug output"
    );
    assert!(
        debug.contains("[redacted]"),
        "should show [redacted] placeholder"
    );
}

#[test]
fn http_config_with_rate_limit() {
    use std::time::Duration;

    let config = ImageGenHttpConfig::new("key").with_rate_limit(limit3r::RateLimitConfig {
        limit_for_period: 10,
        limit_refresh_period: Duration::from_secs(60),
        timeout_duration: Duration::from_secs(120),
    });
    assert!(config.rate_limiter.is_some());
    assert!(config.rate_limit_config.is_some());
}
