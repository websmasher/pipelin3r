//! In-memory adapters for resilience components — delegates to limit3r.

pub use limit3r::{InMemoryBulkhead, InMemoryCircuitBreaker, InMemoryRateLimiter, TokioRetryExecutor};
