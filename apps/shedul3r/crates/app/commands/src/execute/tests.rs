use super::*;
use core::future::Future;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

use tokio::sync::{Notify, Semaphore};

// ── Mock SubprocessRunner ──────────────────────────────────────────

/// Mock subprocess runner that blocks until a `Notify` is triggered,
/// then returns a successful result.
struct MockSubprocess {
    notify: Arc<Notify>,
}

impl SubprocessRunner for MockSubprocess {
    fn run(
        &self,
        _command: SubprocessCommand,
    ) -> impl Future<Output = Result<SubprocessResult, SchedulrError>> + Send {
        let notify = Arc::clone(&self.notify);
        async move {
            notify.notified().await;
            Ok(SubprocessResult {
                exit_code: 0,
                stdout: "ok".to_owned(),
                stderr: String::new(),
                elapsed: Duration::ZERO,
            })
        }
    }
}

// ── Mock RateLimiter ───────────────────────────────────────────────

/// Mock rate limiter that always permits.
struct MockRateLimiter;

impl RateLimiter for MockRateLimiter {
    async fn acquire_permission(
        &self,
        _key: &str,
        _config: &domain_types::RateLimitConfig,
    ) -> Result<(), limit3r::Limit3rError> {
        Ok(())
    }
}

// ── Mock CircuitBreaker ────────────────────────────────────────────

/// Mock circuit breaker that always permits and ignores outcomes.
struct MockCircuitBreaker;

impl CircuitBreaker for MockCircuitBreaker {
    fn check_permitted(
        &self,
        _key: &str,
        _config: &CircuitBreakerConfig,
    ) -> Result<(), limit3r::Limit3rError> {
        Ok(())
    }

    fn record_success(&self, _key: &str) {}
    fn record_failure(&self, _key: &str) {}
}

// ── Mock Bulkhead ──────────────────────────────────────────────────

/// Mock bulkhead backed by a real semaphore for permit tracking.
struct MockBulkhead {
    semaphore: Arc<Semaphore>,
    acquire_count: AtomicU32,
    release_count: AtomicU32,
}

impl MockBulkhead {
    fn new(permits: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            acquire_count: AtomicU32::new(0),
            release_count: AtomicU32::new(0),
        }
    }
}

impl Bulkhead for MockBulkhead {
    fn acquire(
        &self,
        _key: &str,
        _config: &domain_types::BulkheadConfig,
    ) -> impl Future<Output = Result<(), limit3r::Limit3rError>> + Send {
        let sem = Arc::clone(&self.semaphore);
        // Increment acquire count before the future resolves.
        // We do it eagerly here so it's visible to the test immediately.
        let _ = self.acquire_count.fetch_add(1, Ordering::SeqCst);
        async move {
            // acquire_owned takes ownership of the permit; we forget it
            // because release is handled by our guard / manual release call.
            sem.acquire_owned()
                .await
                .map_err(|_| limit3r::Limit3rError::BulkheadFull {
                    key: "mock".to_owned(),
                })?
                .forget();
            Ok(())
        }
    }

    fn release(&self, _key: &str) {
        let _ = self.release_count.fetch_add(1, Ordering::SeqCst);
        self.semaphore.add_permits(1);
    }
}

// ── Mock RetryExecutor ─────────────────────────────────────────────

/// Mock retry executor that runs the operation once (no retry).
struct MockRetryExecutor;

impl RetryExecutor for MockRetryExecutor {
    async fn execute_with_retry<F, Fut, T, E>(
        &self,
        operation: F,
        _config: &domain_types::RetryConfig,
    ) -> Result<T, E>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, E>> + Send,
        T: Send,
        E: From<limit3r::Limit3rError> + Send,
    {
        operation().await
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// YAML task definition with a bulkhead configured (max 1 concurrent).
const TASK_YAML_WITH_BULKHEAD: &str = "\
name: test-task\n\
command: echo hello\n\
provider-id: test-key\n\
max-concurrent: 1\n";

/// Build a [`TaskRequest`] from the bulkhead YAML fixture.
fn bulkhead_request() -> TaskRequest {
    TaskRequest {
        task: TASK_YAML_WITH_BULKHEAD.to_owned(),
        input: None,
        limiter_key: None,
        working_directory: None,
        environment: None,
        timeout_ms: None,
    }
}

/// Concrete engine type used by all tests.
type TestEngine = TaskEngine<
    MockSubprocess,
    MockRateLimiter,
    MockCircuitBreaker,
    MockBulkhead,
    MockRetryExecutor,
>;

/// Shared engine + bulkhead fixture for permit-tracking tests.
struct TestHarness {
    engine: TestEngine,
    bulkhead: Arc<MockBulkhead>,
    subprocess_notify: Arc<Notify>,
}

/// Build a [`TestHarness`] with 1 bulkhead permit.
fn test_harness() -> TestHarness {
    let subprocess_notify = Arc::new(Notify::new());
    let bulkhead = Arc::new(MockBulkhead::new(1));
    let engine = TaskEngine::new(
        Arc::new(MockSubprocess {
            notify: Arc::clone(&subprocess_notify),
        }),
        Arc::new(MockRateLimiter),
        Arc::new(MockCircuitBreaker),
        Arc::clone(&bulkhead),
        Arc::new(MockRetryExecutor),
    );
    TestHarness {
        engine,
        bulkhead,
        subprocess_notify,
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn cancellation_releases_bulkhead_permit() {
    let h = test_harness();

    // Spawn the task but cancel it via timeout before it completes.
    let execute_fut = h.engine.execute(bulkhead_request());
    let timed_out = tokio::time::timeout(Duration::from_millis(50), execute_fut).await;

    // The future should have been cancelled (timed out).
    assert!(timed_out.is_err(), "execute should have timed out");

    // The bulkhead permit must be released despite cancellation.
    assert_eq!(
        h.bulkhead.release_count.load(Ordering::SeqCst),
        1,
        "bulkhead permit should be released after cancellation"
    );
    assert_eq!(
        h.bulkhead.semaphore.available_permits(),
        1,
        "semaphore should have 1 permit available after cancellation"
    );
}

#[tokio::test]
async fn normal_completion_releases_bulkhead_permit() {
    let h = test_harness();

    // Let the subprocess complete immediately.
    h.subprocess_notify.notify_one();

    let result = h.engine.execute(bulkhead_request()).await;
    assert!(result.is_ok(), "execute should succeed");

    let response = result.ok();
    assert!(
        response.as_ref().is_some_and(|r| r.success),
        "task should report success"
    );

    // The bulkhead permit must be released after normal completion.
    assert_eq!(
        h.bulkhead.release_count.load(Ordering::SeqCst),
        1,
        "bulkhead permit should be released after normal completion"
    );
    assert_eq!(
        h.bulkhead.semaphore.available_permits(),
        1,
        "semaphore should have 1 permit available after normal completion"
    );
}
