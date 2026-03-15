//! Shared application state for Axum handlers.

use std::sync::Arc;

use commands::TaskEngine;
use state::{InMemoryBulkhead, InMemoryCircuitBreaker, InMemoryRateLimiter, TokioRetryExecutor};
use subprocess::TokioSubprocessRunner;

/// Concrete `TaskEngine` type with all in-memory adapters resolved.
pub type ConcreteEngine = TaskEngine<
    TokioSubprocessRunner,
    InMemoryRateLimiter,
    InMemoryCircuitBreaker,
    InMemoryBulkhead,
    TokioRetryExecutor,
>;

/// Shared state injected into all Axum handlers via `State<Arc<AppState>>`.
pub struct AppState {
    /// The task execution engine with all adapters wired.
    pub engine: ConcreteEngine,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("engine", &"TaskEngine<...>")
            .finish()
    }
}

/// Creates a new [`AppState`] with default in-memory adapters.
pub fn build_app_state() -> Arc<AppState> {
    let subprocess = Arc::new(TokioSubprocessRunner::new());
    let rate_limiter = Arc::new(InMemoryRateLimiter::new());
    let circuit_breaker = Arc::new(InMemoryCircuitBreaker::new());
    let bulkhead = Arc::new(InMemoryBulkhead::new());
    let retry_executor = Arc::new(TokioRetryExecutor::new());

    let engine = TaskEngine::new(
        subprocess,
        rate_limiter,
        circuit_breaker,
        bulkhead,
        retry_executor,
    );

    Arc::new(AppState { engine })
}
