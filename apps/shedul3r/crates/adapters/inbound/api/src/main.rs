//! Server binary entry point: starts the Axum HTTP server or runs a CLI command.
#![allow(unused_crate_dependencies)] // bin+lib crate: lib.rs owns dependency tracking

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, middleware, routing::get};
use clap::Parser;
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;

use api::bundle_router;
use api::cli::Cli;
use api::state::build_app_state;
use api::task_router;

/// Health check handler — returns 200 "ok".
async fn health() -> &'static str {
    "ok"
}

fn main() {
    let cli = Cli::parse();

    if cli.command.is_some() {
        // CLI mode: run the command and exit with appropriate code.
        let exit_code = api::cli::run_cli(cli);
        #[allow(clippy::disallowed_methods)] // CLI must set exit code
        std::process::exit(exit_code);
    }

    // Daemon mode: start the HTTP server.
    run_server(cli.port);
}

/// Starts the Axum HTTP daemon with all routes wired.
fn run_server(port_override: Option<u16>) {
    #[allow(clippy::expect_used)] // Startup: tokio runtime creation failure is unrecoverable
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async { serve(port_override).await });
}

/// Maximum request body size for task endpoints (50 MB).
const TASK_BODY_LIMIT: usize = 50_000_000;

/// Maximum request body size for bundle uploads (100 MB).
const BUNDLE_BODY_LIMIT: usize = 100_000_000;

/// Async server setup and run loop.
async fn serve(port_override: Option<u16>) {
    // Initialize structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    // Port priority: --port flag > PORT env var > default 7943.
    let port = match port_override {
        Some(p) => p.to_string(),
        None => {
            #[allow(clippy::disallowed_methods)] // Startup: env var reading is confined to main()
            let env_port = std::env::var("PORT").unwrap_or_else(|_| "7943".to_owned());
            env_port
        }
    };

    let addr = format!("0.0.0.0:{port}");

    let state = build_app_state();

    // Per-route body limits: task endpoints get 50MB, bundle upload gets 100MB.
    let mut app = task_router()
        .layer(DefaultBodyLimit::max(TASK_BODY_LIMIT))
        .merge(
            bundle_router().layer(DefaultBodyLimit::max(BUNDLE_BODY_LIMIT)),
        )
        .route("/health", get(health))
        .with_state(Arc::clone(&state))
        // Panic-to-HTTP conversion — prevents raw panic messages leaking to clients
        .layer(CatchPanicLayer::custom(|_| {
            let body = serde_json::json!({
                "error": "internal_server_error",
                "message": "An unexpected error occurred"
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
        }));

    // Optional API key authentication: if SHEDUL3R_API_KEY is set, require
    // Bearer token on all requests.
    #[allow(clippy::disallowed_methods)] // Startup: env var reading is confined to main()
    if let Ok(key) = std::env::var("SHEDUL3R_API_KEY") {
        tracing::info!("API key authentication enabled");
        app = app.layer(middleware::from_fn(move |req, next| {
            let k = key.clone();
            api::auth::check_api_key(req, next, k)
        }));
    } else {
        tracing::warn!("SHEDUL3R_API_KEY not set — running without authentication");
    }

    #[allow(clippy::expect_used)] // Startup: binding failure is unrecoverable
    let listener = TcpListener::bind(&addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!("listening on {addr}");

    #[allow(clippy::expect_used)] // Startup: server failure is unrecoverable
    axum::serve(listener, app).await.expect("server error");
}
