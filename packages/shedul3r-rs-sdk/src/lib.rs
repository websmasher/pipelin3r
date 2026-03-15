//! Rust client SDK for the shedul3r task execution server.

// Suppress unused-crate-dependencies for stub modules.
use anyhow as _;
use reqwest as _;
use serde as _;
use serde_json as _;
use tokio as _;
use tracing as _;

/// HTTP client for the shedul3r API.
pub mod client;
/// Bundle upload and download utilities.
pub mod bundle;
