//! Typed error enum for the pipelin3r pipeline orchestrator.

/// Errors that can occur during pipeline execution.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// An error from the shedul3r SDK.
    #[error("SDK error: {0}")]
    Sdk(#[from] shedul3r_rs_sdk::SdkError),
    /// Authentication configuration is missing or invalid.
    #[error("auth error: {0}")]
    Auth(String),
    /// Template rendering failed.
    #[error("template error: {0}")]
    Template(String),
    /// Bundle creation or extraction failed.
    #[error("bundle error: {0}")]
    Bundle(String),
    /// A shell command failed.
    #[error("command failed: {0}")]
    Command(String),
    /// A data transform failed.
    #[error("transform error: {0}")]
    Transform(String),
    /// Configuration is invalid.
    #[error("config error: {0}")]
    Config(String),
    /// A filesystem I/O operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Catch-all for errors that do not fit other variants.
    #[error("{0}")]
    Other(String),
}
