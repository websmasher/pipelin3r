//! Image generation via the `OpenRouter` API.
//!
//! This module provides a standalone `generate_image()` function that calls
//! the `OpenRouter` chat completions endpoint with image modalities. It does
//! **not** go through the [`Executor`](crate::executor::Executor) or shedul3r
//! -- it makes direct HTTP calls.
//!
//! # Usage
//!
//! ```rust,no_run
//! use pipelin3r::image_gen::{
//!     generate_image, ImageGenConfig, ImageGenHttpConfig,
//!     AspectRatio, ImageModel,
//! };
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), pipelin3r::PipelineError> {
//! let http = ImageGenHttpConfig::from_env()?;
//! let config = ImageGenConfig::new("A sunset over mountains", PathBuf::from("./output"));
//! let result = generate_image(&http, &config).await?;
//! result.require_success()?;
//! # Ok(())
//! # }
//! ```

mod client;
/// Type definitions for image generation requests and responses.
pub mod types;

use std::path::PathBuf;
use std::sync::Arc;

use limit3r::{InMemoryRateLimiter, RateLimitConfig, RateLimiter as _};

use crate::error::PipelineError;

pub use types::{AspectRatio, ImageModel, RefImage, RefImageRole};

// ── HTTP configuration (shared across calls) ──

/// Shared HTTP and rate-limiting configuration for image generation.
///
/// Holds the HTTP client, API key, and optional rate limiter. Create once
/// and pass by reference to [`generate_image()`] for each call.
pub struct ImageGenHttpConfig {
    /// Reusable HTTP client (connection pooling).
    http_client: reqwest::Client,
    /// `OpenRouter` API key.
    api_key: String,
    /// Optional in-process rate limiter instance.
    rate_limiter: Option<Arc<InMemoryRateLimiter>>,
    /// Rate limit configuration (required if `rate_limiter` is set).
    rate_limit_config: Option<RateLimitConfig>,
}

#[allow(
    clippy::missing_fields_in_debug,
    reason = "api_key is intentionally redacted, http_client and rate_limit_config are internal details"
)]
impl std::fmt::Debug for ImageGenHttpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageGenHttpConfig")
            .field("api_key", &"[redacted]")
            .field("has_rate_limiter", &self.rate_limiter.is_some())
            .finish()
    }
}

impl ImageGenHttpConfig {
    /// Create a new HTTP config with the given API key.
    #[allow(
        clippy::disallowed_methods,
        reason = "reqwest::Client::new() is the standard constructor"
    )]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            api_key: api_key.into(),
            rate_limiter: None,
            rate_limit_config: None,
        }
    }

    /// Create a new HTTP config by reading `OPENROUTER_API_KEY` from the
    /// environment.
    ///
    /// # Errors
    /// Returns an error if the environment variable is not set.
    #[allow(
        clippy::disallowed_methods,
        reason = "reading env var is core functionality for from_env()"
    )]
    pub fn from_env() -> Result<Self, PipelineError> {
        let api_key = std::env::var("OPENROUTER_API_KEY").map_err(|_| {
            PipelineError::Config(String::from(
                "OPENROUTER_API_KEY environment variable is not set",
            ))
        })?;
        Ok(Self::new(api_key))
    }

    /// Enable in-process rate limiting with the given configuration.
    ///
    /// A new [`InMemoryRateLimiter`] is created and shared across all calls
    /// using this config instance.
    #[must_use]
    pub fn with_rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.rate_limiter = Some(Arc::new(InMemoryRateLimiter::new()));
        self.rate_limit_config = Some(config);
        self
    }

    /// Enable in-process rate limiting with a pre-existing limiter instance.
    ///
    /// Useful when sharing a single limiter across multiple config instances.
    #[must_use]
    pub fn with_shared_rate_limiter(
        mut self,
        limiter: Arc<InMemoryRateLimiter>,
        config: RateLimitConfig,
    ) -> Self {
        self.rate_limiter = Some(limiter);
        self.rate_limit_config = Some(config);
        self
    }
}

// ── Per-call configuration ──

/// Configuration for a single image generation call.
#[derive(Debug, Clone)]
pub struct ImageGenConfig {
    /// Image model to use.
    pub model: ImageModel,
    /// Text prompt describing the desired image.
    pub prompt: String,
    /// Reference images attached to the request.
    pub reference_images: Vec<RefImage>,
    /// Desired aspect ratio.
    pub aspect_ratio: AspectRatio,
    /// Directory where the output image will be written.
    pub work_dir: PathBuf,
    /// Filename for the output image (within `work_dir`).
    pub output_filename: String,
}

impl ImageGenConfig {
    /// Create a new config with required fields and sensible defaults.
    ///
    /// Defaults: `Gemini2_5Flash` model, `Square` aspect ratio,
    /// output filename `"generated.png"`.
    pub fn new(prompt: impl Into<String>, work_dir: PathBuf) -> Self {
        Self {
            model: ImageModel::default(),
            prompt: prompt.into(),
            reference_images: Vec::new(),
            aspect_ratio: AspectRatio::default(),
            work_dir,
            output_filename: String::from("generated.png"),
        }
    }
}

// ── Result ──

/// Result of an image generation call.
#[derive(Debug, Clone)]
pub struct ImageGenResult {
    /// Whether the generation completed successfully.
    pub success: bool,
    /// Paths to generated output files.
    pub output_files: Vec<PathBuf>,
    /// API cost in USD (if reported by the provider).
    pub cost: Option<f64>,
    /// MIME type of the generated image.
    pub output_mime: Option<String>,
}

impl ImageGenResult {
    /// Return a reference to self if successful, or an error if not.
    ///
    /// # Errors
    /// Returns [`PipelineError::ImageGenFailed`] if `success` is `false`.
    pub fn require_success(&self) -> Result<&Self, PipelineError> {
        if self.success {
            Ok(self)
        } else {
            Err(PipelineError::ImageGenFailed {
                message: String::from("image generation did not succeed"),
            })
        }
    }
}

// ── Public entry point ──

/// Rate limiter key for `OpenRouter` API calls.
const RATE_LIMIT_KEY: &str = "openrouter-image-gen";

/// Generate an image via the `OpenRouter` API.
///
/// 1. If a rate limiter is configured, acquires a permit (blocking until available).
/// 2. Builds the request body with image modalities and reference images.
/// 3. POSTs to the `OpenRouter` chat completions endpoint.
/// 4. Parses the response, decodes the base64 image data.
/// 5. Writes the image to `config.work_dir / config.output_filename`.
/// 6. If cost is 0/absent and a generation ID exists, does a follow-up cost lookup.
/// 7. Returns [`ImageGenResult`].
///
/// # Errors
/// Returns [`PipelineError::ImageGenFailed`] if any step fails.
pub async fn generate_image(
    http: &ImageGenHttpConfig,
    config: &ImageGenConfig,
) -> Result<ImageGenResult, PipelineError> {
    // Step 1: Rate limiting.
    if let (Some(limiter), Some(rl_config)) = (&http.rate_limiter, &http.rate_limit_config) {
        limiter
            .acquire_permission(RATE_LIMIT_KEY, rl_config)
            .await
            .map_err(|e| PipelineError::ImageGenFailed {
                message: format!("rate limit: {e}"),
            })?;
    }

    // Step 2: Build request body.
    let request = client::generate_image_request(
        &config.model,
        &config.prompt,
        &config.reference_images,
        &config.aspect_ratio,
    );

    // Step 3: Send HTTP request.
    let response = client::send_image_request(&http.http_client, &http.api_key, &request).await?;

    // Step 4: Parse response and decode image.
    let output_path = config.work_dir.join(&config.output_filename);
    let generated = client::parse_image_response(&response)?;

    // Step 5: Ensure work_dir exists and write the image.
    #[allow(
        clippy::disallowed_methods,
        reason = "image output requires direct filesystem write to work_dir"
    )]
    if !config.work_dir.exists() {
        crate::fs::create_dir_all(&config.work_dir).map_err(|e| {
            PipelineError::Transport(format!(
                "failed to create work_dir {}: {e}",
                config.work_dir.display()
            ))
        })?;
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "image output requires direct filesystem write to work_dir"
    )]
    crate::fs::write(&output_path, &generated.data).map_err(|e| {
        PipelineError::Transport(format!(
            "failed to write image to {}: {e}",
            output_path.display()
        ))
    })?;

    // Step 6: Follow-up cost lookup if needed.
    let mut cost = generated.cost;
    let cost_is_missing = cost.is_none() || cost == Some(0.0);
    if cost_is_missing {
        if let Some(ref gen_id) = generated.generation_id {
            if let Some(fetched) =
                client::fetch_cost(&http.http_client, &http.api_key, gen_id).await
            {
                cost = Some(fetched);
            }
        }
    }

    // Step 7: Return result.
    Ok(ImageGenResult {
        success: true,
        output_files: vec![output_path],
        cost,
        output_mime: Some(generated.mime),
    })
}

#[cfg(test)]
mod tests;
