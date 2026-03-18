//! Shared application state for HTTP handlers.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use std::time::Instant;

use commands::TaskEngine;
use domain_types::{AsyncTaskState, AsyncTaskStatus};
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

/// A stored async task entry with its current state and completion timestamp.
#[derive(Debug)]
pub struct AsyncTaskEntry {
    /// Current state of the task.
    state: AsyncTaskState,
    /// When the entry was completed or failed (used for TTL reaping).
    /// `None` while the task is still running — running tasks are never reaped.
    completed_at: Option<Instant>,
}

/// Time-to-live for completed/failed async task entries (10 minutes).
const TTL_SECS: u64 = 600;

/// In-memory store for async task results.
///
/// Tasks are inserted as `Running`, then marked `Completed` or `Failed`
/// by the background executor. Completed/failed entries are reaped after
/// [`TTL_SECS`] seconds.
pub struct AsyncTaskStore {
    /// Map from task ID to entry.
    tasks: RwLock<BTreeMap<String, AsyncTaskEntry>>,
}

impl std::fmt::Debug for AsyncTaskStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncTaskStore")
            .field("task_count", &self.tasks.read().len())
            .finish()
    }
}

impl AsyncTaskStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(BTreeMap::new()),
        }
    }

    /// Insert a new task in the `Running` state.
    pub fn insert_running(&self, task_id: String) {
        let entry = AsyncTaskEntry {
            state: AsyncTaskState::Running,
            completed_at: None,
        };
        let _prev = self.tasks.write().insert(task_id, entry);
    }

    /// Mark a task as completed with its response.
    #[allow(
        clippy::print_stderr,
        reason = "server diagnostic — log warning for unknown task IDs"
    )]
    pub fn mark_completed(&self, task_id: &str, response: domain_types::TaskResponse) {
        let mut tasks = self.tasks.write();
        if let Some(entry) = tasks.get_mut(task_id) {
            entry.state = AsyncTaskState::Completed(response);
            entry.completed_at = Some(Instant::now());
        } else {
            eprintln!("[shedul3r] WARNING: mark_completed called with unknown task ID: {task_id}");
        }
    }

    /// Mark a task as failed with an error message.
    #[allow(
        clippy::print_stderr,
        reason = "server diagnostic — log warning for unknown task IDs"
    )]
    pub fn mark_failed(&self, task_id: &str, error: String) {
        let mut tasks = self.tasks.write();
        if let Some(entry) = tasks.get_mut(task_id) {
            entry.state = AsyncTaskState::Failed(error);
            entry.completed_at = Some(Instant::now());
        } else {
            eprintln!("[shedul3r] WARNING: mark_failed called with unknown task ID: {task_id}");
        }
    }

    /// Get the current status of a task for client polling.
    pub fn get_status(&self, task_id: &str) -> Option<AsyncTaskStatus> {
        let tasks = self.tasks.read();
        tasks.get(task_id).map(|entry| match &entry.state {
            AsyncTaskState::Running => AsyncTaskStatus {
                status: "running".to_owned(),
                result: None,
                error: None,
            },
            AsyncTaskState::Completed(response) => AsyncTaskStatus {
                status: "completed".to_owned(),
                result: Some(response.clone()),
                error: None,
            },
            AsyncTaskState::Failed(msg) => AsyncTaskStatus {
                status: "failed".to_owned(),
                result: None,
                error: Some(msg.clone()),
            },
        })
    }

    /// Remove completed/failed entries whose `completed_at` timestamp is
    /// older than [`TTL_SECS`].
    ///
    /// Running tasks (which have no `completed_at`) are never reaped.
    /// Returns the number of entries removed.
    pub fn reap_expired(&self) -> usize {
        let now = Instant::now();
        let mut tasks = self.tasks.write();
        let before = tasks.len();
        tasks.retain(|_id, entry| {
            match entry.completed_at {
                None => true, // running tasks have no completed_at — never reap
                Some(completed) => now.duration_since(completed).as_secs() < TTL_SECS,
            }
        });
        before.saturating_sub(tasks.len())
    }
}

/// Shared state injected into all HTTP handlers via `web::Data<Arc<AppState>>`.
pub struct AppState {
    /// The task execution engine with all adapters wired (shared with MCP transport).
    pub engine: Arc<ConcreteEngine>,
    /// In-memory bundle storage for uploaded file bundles.
    pub bundles: BundleStore,
    /// In-memory storage for async task results (submit + poll pattern).
    pub async_tasks: Arc<AsyncTaskStore>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("engine", &"TaskEngine<...>")
            .field("bundles", &"BundleStore")
            .field("async_tasks", &self.async_tasks)
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
        engine: Arc::new(engine),
        bundles: Arc::new(RwLock::new(BTreeMap::new())),
        async_tasks: Arc::new(AsyncTaskStore::new()),
    })
}
