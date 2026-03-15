# limit3r

Resilience patterns for async Rust: rate limiter, circuit breaker, bulkhead, retry.

## Installation

```sh
cargo add limit3r
```

## Quick Start

```rust
use limit3r::{InMemoryRateLimiter, RateLimiter, RateLimitConfig};
use std::time::Duration;

let limiter = InMemoryRateLimiter::new();
let config = RateLimitConfig {
    limit_for_period: 100,
    limit_refresh_period: Duration::from_secs(60),
    timeout_duration: Duration::from_millis(500),
};

// Returns Ok(()) if a permit is available, Err(RateLimitExceeded) otherwise.
limiter.acquire_permission("api-key-123", &config).await?;
```

## Features

- **Rate limiter** -- fixed-window permits per key with configurable refresh period
- **Circuit breaker** -- count-based failure tracking with half-open probe recovery
- **Bulkhead** -- semaphore-based concurrency limiting per key
- **Retry executor** -- exponential backoff with configurable multiplier and max delay
- All implementations are async (tokio) and `Send + Sync`
- Automatic key eviction (max 10,000 tracked keys)
- Config validation methods on all config types
- Serde support for all configuration types (`Serialize` + `Deserialize`)

## License

MIT
