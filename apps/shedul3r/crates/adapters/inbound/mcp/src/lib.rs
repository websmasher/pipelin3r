//! Inbound MCP (Model Context Protocol) adapter for shedul3r.
//!
//! Exposes the task execution engine as MCP tools via Streamable HTTP
//! transport on a separate port from the REST API.

pub mod server;
pub mod service;

// Used by service.rs at runtime.
use axum as _;
use tokio as _;
use tokio_util as _;
