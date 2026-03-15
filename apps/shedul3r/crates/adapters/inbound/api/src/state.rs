//! Shared application state for Axum handlers.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use commands::TaskEngine;
use parking_lot::RwLock;
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

/// A stored bundle: the path to the temp directory and an RAII guard that
/// cleans it up on drop.
#[derive(Debug)]
pub struct BundleEntry {
    /// Filesystem path to the bundle root directory.
    pub path: PathBuf,
    /// Dropping this removes the temp directory from disk.
    pub temp_dir: tempfile::TempDir,
}

impl BundleEntry {
    /// Creates a new bundle entry from a [`tempfile::TempDir`].
    pub fn new(temp_dir: tempfile::TempDir) -> Self {
        let path = temp_dir.path().to_path_buf();
        Self { path, temp_dir }
    }
}

/// Thread-safe in-memory store mapping bundle IDs to their temp directories.
pub type BundleStore = Arc<RwLock<BTreeMap<String, BundleEntry>>>;

/// Shared state injected into all Axum handlers via `State<Arc<AppState>>`.
pub struct AppState {
    /// The task execution engine with all adapters wired.
    pub engine: ConcreteEngine,
    /// In-memory bundle storage for uploaded file bundles.
    pub bundles: BundleStore,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("engine", &"TaskEngine<...>")
            .field("bundles", &"BundleStore")
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

    Arc::new(AppState {
        engine,
        bundles: Arc::new(RwLock::new(BTreeMap::new())),
    })
}
