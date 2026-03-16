//! Inbound HTTP adapter: Axum server, handlers, CLI transport, and error mapping.

pub mod auth;
pub mod cli;
mod error;
mod extractors;
pub(crate) mod handlers;
pub mod state;

// Used by the [[bin]] target (main.rs), not by lib code directly.
use tokio as _;
use tower_http as _;
use tracing_subscriber as _;

pub use error::AppError;
pub use extractors::ValidatedJson;
pub use handlers::bundle_router;
pub use handlers::task_router;
