//! Fixed-window rate limiter backed by in-memory state.

use std::collections::BTreeMap;

use crate::config::RateLimitConfig;
use crate::error::Limit3rError;
use crate::traits::RateLimiter;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Maximum number of keys tracked before stale entries are evicted.
const MAX_TRACKED_KEYS: usize = 10_000;

/// Per-key state tracking permits consumed in the current time window.
struct KeyState {
    /// Number of permits consumed in the current window.
    permits_used: u32,
    /// When the current window started.
    window_start: Instant,
}

/// Type alias to reduce type complexity for the inner state map.
type StateMap = BTreeMap<String, KeyState>;

/// In-memory fixed-window rate limiter.
///
/// Tracks permit usage per key using a fixed time-window counter.
/// When the window expires, the counter resets and permits become available
/// again. If no permits remain in the current window the caller blocks
/// (up to `timeout_duration`) until the window refreshes.
#[derive(Debug)]
pub struct InMemoryRateLimiter {
    state: Mutex<StateMap>,
}

impl Default for InMemoryRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRateLimiter {
    /// Create a new, empty rate limiter.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(BTreeMap::new()),
        }
    }
}

/// Remove keys whose time window has expired, keeping the map bounded.
fn evict_expired_rate_limit_keys(
    map: &mut StateMap,
    config: &RateLimitConfig,
    now: Instant,
) {
    map.retain(|_key, entry| now.duration_since(entry.window_start) < config.limit_refresh_period);
}

impl RateLimiter for InMemoryRateLimiter {
    async fn acquire_permission(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> Result<(), Limit3rError> {
        let deadline = Instant::now()
            .checked_add(config.timeout_duration)
            .ok_or_else(|| Limit3rError::RateLimitExceeded {
                key: key.to_owned(),
            })?;

        loop {
            let sleep_until = {
                let mut map = self.state.lock().await;
                let now = Instant::now();

                // Evict expired windows when the map exceeds the size limit.
                if map.len() > MAX_TRACKED_KEYS {
                    evict_expired_rate_limit_keys(&mut map, config, now);
                }

                let needs_insert = !map.contains_key(key);
                if needs_insert {
                    let _prev = map.insert(
                        key.to_owned(),
                        KeyState {
                            permits_used: 0,
                            window_start: now,
                        },
                    );
                }

                let Some(entry) = map.get_mut(key) else {
                    return Err(Limit3rError::RateLimitExceeded {
                        key: key.to_owned(),
                    });
                };

                // Reset window if expired
                if now.duration_since(entry.window_start) >= config.limit_refresh_period {
                    entry.permits_used = 0;
                    entry.window_start = now;
                }

                if entry.permits_used < config.limit_for_period {
                    entry.permits_used = entry.permits_used.saturating_add(1);
                    return Ok(());
                }

                // No permits available — compute when the window resets
                let next_window = entry
                    .window_start
                    .checked_add(config.limit_refresh_period)
                    .ok_or_else(|| Limit3rError::RateLimitExceeded {
                        key: key.to_owned(),
                    })?;
                drop(map);
                next_window
            };

            // If waiting would exceed our deadline, fail immediately
            if sleep_until > deadline {
                return Err(Limit3rError::RateLimitExceeded {
                    key: key.to_owned(),
                });
            }

            tokio::time::sleep_until(sleep_until).await;
        }
    }
}

// Required to satisfy Debug derive on the outer struct.
impl std::fmt::Debug for KeyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyState")
            .field("permits_used", &self.permits_used)
            .field("window_start", &self.window_start)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_config(limit: u32, refresh: Duration, timeout: Duration) -> RateLimitConfig {
        RateLimitConfig {
            limit_for_period: limit,
            limit_refresh_period: refresh,
            timeout_duration: timeout,
        }
    }

    #[tokio::test]
    async fn acquire_permit_succeeds_when_under_limit() {
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(5, Duration::from_secs(1), Duration::from_millis(100));

        let result = limiter.acquire_permission("key-a", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn acquire_permit_fails_when_limit_exhausted_and_timeout_expires() {
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(1, Duration::from_secs(10), Duration::from_millis(50));

        // Consume the single permit
        limiter.acquire_permission("key-a", &config).await.unwrap();

        // Second acquire should fail because the window won't reset before timeout
        let result = limiter.acquire_permission("key-a", &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn window_resets_after_refresh_period() {
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(1, Duration::from_millis(50), Duration::from_millis(200));

        // Consume the permit
        limiter.acquire_permission("key-a", &config).await.unwrap();

        // Wait for the window to reset
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should succeed again after window refresh
        let result = limiter.acquire_permission("key-a", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn multiple_keys_are_independent() {
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(1, Duration::from_secs(10), Duration::from_millis(50));

        // Exhaust key-a
        limiter.acquire_permission("key-a", &config).await.unwrap();

        // key-b should still succeed
        let result = limiter.acquire_permission("key-b", &config).await;
        assert!(result.is_ok());
    }
}
