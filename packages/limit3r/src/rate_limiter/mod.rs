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
    /// Maximum number of keys tracked before stale entries are evicted.
    max_tracked_keys: usize,
}

impl Default for InMemoryRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRateLimiter {
    /// Create a new, empty rate limiter with the default key limit (10,000).
    pub fn new() -> Self {
        Self::with_max_keys(MAX_TRACKED_KEYS)
    }

    /// Create a new, empty rate limiter with a custom maximum number of tracked keys.
    pub fn with_max_keys(max: usize) -> Self {
        Self {
            state: Mutex::new(BTreeMap::new()),
            max_tracked_keys: max,
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

                // Ensure the current key exists before eviction so it
                // is never a victim.
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

                // Evict expired windows when the map exceeds the size limit,
                // but never evict the current key.
                if map.len() > self.max_tracked_keys {
                    map.retain(|k, state| {
                        k == key
                            || now.duration_since(state.window_start) < config.limit_refresh_period
                    });
                }

                // If still over limit after evicting expired, remove oldest
                // entries excluding the current key.
                if map.len() > self.max_tracked_keys {
                    let excess = map.len().saturating_sub(self.max_tracked_keys);
                    let mut candidates: Vec<_> = map
                        .iter()
                        .filter(|(k, _)| k.as_str() != key)
                        .map(|(k, v)| (k.clone(), v.window_start))
                        .collect();
                    candidates.sort_by_key(|(_, start)| *start);
                    for (evict_key, _) in candidates.into_iter().take(excess) {
                        let _removed = map.remove(&evict_key);
                    }
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
mod tests;
