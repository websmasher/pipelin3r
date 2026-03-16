//! MCP Streamable HTTP service builder.
//!
//! Provides [`build_mcp_router`] which creates an Axum router handling MCP
//! protocol sessions over Streamable HTTP transport, and [`serve_mcp`] which
//! runs it on a separate port.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio_util::sync::CancellationToken;

use crate::server::{ConcreteEngine, McpTaskServer};

/// Build an Axum router serving the MCP Streamable HTTP endpoint.
pub fn build_mcp_router(
    engine: Arc<ConcreteEngine>,
    cancellation_token: CancellationToken,
) -> axum::Router {
    let service = StreamableHttpService::new(
        move || Ok(McpTaskServer::new(Arc::clone(&engine))),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            cancellation_token,
            ..Default::default()
        },
    );

    axum::Router::new().nest_service("/mcp", service)
}

/// Run the MCP server on the given address.
///
/// This is a separate Axum instance from the REST API. Call it from
/// a spawned task alongside the REST server.
///
/// # Errors
///
/// Returns an error if binding or serving fails.
pub async fn serve_mcp(
    engine: Arc<ConcreteEngine>,
    addr: &str,
    cancellation_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let router = build_mcp_router(engine, cancellation_token);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("MCP transport listening on {addr}");
    axum::serve(listener, router).await?;

    Ok(())
}
