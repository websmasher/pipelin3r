//! Outbound port traits: interfaces for subprocess execution and resilience components.

use core::future::Future;

use domain_types::{SchedulrError, SubprocessCommand, SubprocessResult};

// Re-export resilience traits from limit3r so consumers import from `repo::`.
pub use limit3r::{Bulkhead, CircuitBreaker, RateLimiter, RetryExecutor};

/// Runs shell commands as subprocesses.
pub trait SubprocessRunner: Send + Sync {
    /// Execute the given command and return its result.
    fn run(
        &self,
        command: SubprocessCommand,
    ) -> impl Future<Output = Result<SubprocessResult, SchedulrError>> + Send;
}
