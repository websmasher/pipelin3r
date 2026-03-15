//! Sliding-window token bucket rate limiter backed by in-memory state.

use std::collections::BTreeMap;

use crate::config::RateLimitConfig;
use crate::error::Limit3rError;
use crate::traits::RateLimiter;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Per-key state tracking permits consumed in the current time window.
struct KeyState {
    /// Number of permits consumed in the current window.
    permits_used: u32,
    /// When the current window started.
    window_start: Instant,
}

/// Type alias to reduce type complexity for the inner state map.
type StateMap = BTreeMap<String, KeyState>;

/// In-memory sliding-window rate limiter.
///
/// Tracks permit usage per key using a time-windowed counter.
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
