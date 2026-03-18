//! Server binary entry point: starts the actix-web HTTP server or runs a CLI command.
#![allow(unused_crate_dependencies)] // bin+lib crate: lib.rs owns dependency tracking

use std::sync::Arc;

use actix_web::web;
use actix_web::{App, HttpResponse, HttpServer};
use clap::Parser;
use tokio_util::sync::CancellationToken;

use rest::auth::ApiKeyAuth;
use rest::cli::Cli;
use rest::configure_async_task_routes;
use rest::configure_bundle_routes;
use rest::configure_task_routes;
use rest::state::build_app_state;

/// Health check handler — returns 200 "ok".
async fn health() -> &'static str {
    "ok"
}

fn main() {
    let cli = Cli::parse();

    if cli.command.is_some() {
        // CLI mode: run the command and exit with appropriate code.
        let exit_code = rest::cli::run_cli(cli);
        #[allow(clippy::disallowed_methods)] // CLI must set exit code
        std::process::exit(exit_code);
    }

    // Daemon mode: start the HTTP server.
    run_server(cli.port);
}

/// Starts the actix-web HTTP daemon with all routes wired.
fn run_server(port_override: Option<u16>) {
    #[allow(clippy::expect_used)] // Startup: tokio runtime creation failure is unrecoverable
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async { serve(port_override).await });
}

/// Maximum request body size for task endpoints (50 MB).
const TASK_BODY_LIMIT: usize = 50_000_000;

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

    // Start MCP transport on a separate port (REST port + 1).
    let mcp_port = port
        .parse::<u16>()
        .unwrap_or(7943)
        .checked_add(1)
        .unwrap_or(7944);
    let mcp_addr = format!("0.0.0.0:{mcp_port}");
    let mcp_engine = Arc::clone(&state.engine);
    let cancellation_token = CancellationToken::new();
    let _mcp_handle = tokio::spawn({
        let addr = mcp_addr.clone();
        let token = cancellation_token.clone();
        async move {
            if let Err(e) = mcp::service::serve_mcp(mcp_engine, &addr, token).await {
                tracing::error!("MCP transport error: {e}");
            }
        }
    });

    // Background reaper: clean up completed/failed async task entries every 60s.
    let reaper_store = Arc::clone(&state.async_tasks);
    let _reaper_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let reaped = reaper_store.reap_expired();
            if reaped > 0 {
                tracing::info!(reaped, "reaped expired async task entries");
            }
        }
    });

    // Optional API key authentication: if SHEDUL3R_API_KEY is set, require
    // Bearer token on protected routes only.
    #[allow(clippy::disallowed_methods)] // Startup: env var reading is confined to main()
    let api_key = std::env::var("SHEDUL3R_API_KEY").ok();

    if api_key.is_some() {
        tracing::info!("API key authentication enabled");
    } else {
        tracing::warn!("SHEDUL3R_API_KEY not set — running without authentication");
    }

    tracing::info!("listening on {addr}");

    #[allow(clippy::expect_used)] // Startup: server binding failure is unrecoverable
    HttpServer::new(move || {
        let mut app = App::new()
            .app_data(web::Data::new(Arc::clone(&state)))
            .app_data(
                web::JsonConfig::default()
                    .limit(TASK_BODY_LIMIT)
                    .error_handler(|err, _req| {
                        let body = serde_json::json!({
                            "error": "bad_request",
                            "message": err.to_string(),
                        });
                        actix_web::error::InternalError::from_response(
                            err,
                            HttpResponse::BadRequest().json(body),
                        )
                        .into()
                    }),
            )
            // Unauthenticated routes: health checks must pass without an API key
            // so that load balancers and readiness probes work regardless of auth config.
            .route("/health", web::get().to(health));

        // Protected routes: task and bundle endpoints that carry business data.
        // Wrapped in an auth-gated scope when SHEDUL3R_API_KEY is set.
        if let Some(ref key) = api_key {
            app = app.service(
                web::scope("")
                    .wrap(ApiKeyAuth::new(key.clone()))
                    .configure(configure_task_routes)
                    .configure(configure_async_task_routes)
                    .configure(configure_bundle_routes),
            );
        } else {
            app = app
                .configure(configure_task_routes)
                .configure(configure_async_task_routes)
                .configure(configure_bundle_routes);
        }

        app
    })
    // Tasks run for minutes (agent executions). Actix-web defaults are 5 seconds
    // for keep-alive and client_request_timeout, which causes "error decoding
    // response body" on the client for any task longer than 5s.
    .keep_alive(std::time::Duration::from_secs(3600))
    .client_request_timeout(std::time::Duration::from_secs(3600))
    .bind(&addr)
    .expect("failed to bind HTTP server")
    .run()
    .await
    .expect("server error");
}
