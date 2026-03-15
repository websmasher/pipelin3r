//! Pipeline orchestration for LLM-powered workflows.

// Suppress unused-crate-dependencies for stub modules.
use anyhow as _;
use serde as _;
use serde_json as _;
use shedul3r_rs_sdk as _;
use tokio as _;
use toml as _;
use tracing as _;

/// Step executor for running pipeline stages.
pub mod executor;
/// Agent interaction and management.
pub mod agent;
/// Command definitions and parsing.
pub mod command;
/// Data transformation utilities.
pub mod transform;
/// Bundle packaging and extraction.
pub mod bundle;
/// Template rendering and interpolation.
pub mod template;
/// Authentication and authorization.
pub mod auth;
