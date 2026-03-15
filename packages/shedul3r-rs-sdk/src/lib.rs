//! Rust client SDK for the shedul3r task execution server.
//!
//! This crate provides a typed HTTP client for shedul3r's REST API. It handles
//! task submission, response parsing, and file-poll recovery for long-running
//! tasks where HTTP connections may drop.
//!
//! # Quick start
//!
//! ```no_run
//! use shedul3r_rs_sdk::{Client, ClientConfig, TaskPayload};
//!
//! # async fn example() -> Result<(), shedul3r_rs_sdk::SdkError> {
//! let client = Client::with_defaults()?;
//! let payload = TaskPayload {
//!     task: String::from("name: my-task\ncommand: echo"),
//!     input: String::from("hello"),
//!     working_directory: None,
//!     environment: None,
//!     limiter_key: None,
//!     timeout_ms: None,
//! };
//! let result = client.submit_task(&payload).await?;
//! assert!(result.success);
//! # Ok(())
//! # }
//! ```

/// HTTP client for the shedul3r API.
pub mod client;
/// Bundle upload and download utilities.
pub mod bundle;
/// Typed error enum.
pub mod error;

pub use client::{Client, ClientConfig, TaskPayload, TaskResult};
pub use bundle::BundleHandle;
pub use error::SdkError;
