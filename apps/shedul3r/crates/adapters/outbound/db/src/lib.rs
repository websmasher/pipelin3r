//! In-memory adapters for resilience components — delegates to limit3r.

pub use limit3r::{
    InMemoryBulkhead, InMemoryCircuitBreaker, InMemoryRateLimiter, TokioRetryExecutor,
};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests;
