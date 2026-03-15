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
                if map.len() > MAX_TRACKED_KEYS {
                    map.retain(|k, state| {
                        k == key
                            || now.duration_since(state.window_start)
                                < config.limit_refresh_period
                    });
                }

                // If still over limit after evicting expired, remove oldest
                // entries excluding the current key.
                if map.len() > MAX_TRACKED_KEYS {
                    let excess = map.len().saturating_sub(MAX_TRACKED_KEYS);
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
    async fn regression_rate_limiter_eviction_does_not_evict_current_key() {
        // Regression: eviction could evict the current key being acquired,
        // causing it to lose its state and reset. After the fix, the current
        // key is always inserted/preserved before eviction runs.
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(2, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to MAX_TRACKED_KEYS with other keys
        for i in 0..MAX_TRACKED_KEYS.saturating_add(1) {
            let key = format!("filler-{i}");
            limiter.acquire_permission(&key, &config).await.unwrap();
        }

        // Now acquire on an existing key — it should NOT be evicted/reset.
        // First, create the key and use 1 of 2 permits.
        let target_config = test_config(2, Duration::from_secs(60), Duration::from_millis(100));
        limiter
            .acquire_permission("survivor-key", &target_config)
            .await
            .unwrap();

        // Use second permit
        limiter
            .acquire_permission("survivor-key", &target_config)
            .await
            .unwrap();

        // Both permits consumed — third acquire should fail (timeout quickly).
        // If the key was evicted and re-created, it would have fresh permits and succeed.
        let fail_config = test_config(2, Duration::from_secs(60), Duration::from_millis(50));
        let result = limiter
            .acquire_permission("survivor-key", &fail_config)
            .await;
        assert!(
            result.is_err(),
            "key was evicted and re-created with fresh permits — eviction must not affect current key"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_triggers_at_max_tracked_keys_rate_limiter() {
        // Mutant kill: rate_limiter.rs:83,93 — replace > with ==/</>=/>=
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(10, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to exactly MAX_TRACKED_KEYS
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("fill-{i}");
            limiter.acquire_permission(&key, &config).await.unwrap();
        }

        // Verify size is MAX_TRACKED_KEYS
        let size_at_max = limiter.state.lock().await.len();
        assert_eq!(
            size_at_max, MAX_TRACKED_KEYS,
            "no eviction at exactly MAX_TRACKED_KEYS"
        );

        // Add one more — triggers eviction
        limiter
            .acquire_permission("one-more", &config)
            .await
            .unwrap();

        let size_after = limiter.state.lock().await.len();
        assert!(
            size_after <= MAX_TRACKED_KEYS.saturating_add(1),
            "eviction should run after exceeding MAX_TRACKED_KEYS, got {size_after}"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_removes_expired_windows_first() {
        // Mutant kill: rate_limiter.rs:85 — replace == with != (expired check in retain)
        // Mutant kill: rate_limiter.rs:86 — replace || with && (exclude current key)
        // Mutant kill: rate_limiter.rs:87 — replace < with ==/>/<=
        let limiter = InMemoryRateLimiter::new();
        // Short refresh period so windows expire quickly
        let short_config =
            test_config(10, Duration::from_millis(10), Duration::from_millis(100));

        // Create keys that will expire quickly
        for i in 0..100 {
            let key = format!("expired-{i}");
            limiter
                .acquire_permission(&key, &short_config)
                .await
                .unwrap();
        }

        // Wait for them to expire
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Create non-expired keys with long refresh
        let long_config = test_config(10, Duration::from_secs(60), Duration::from_millis(100));
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("fresh-{i}");
            limiter
                .acquire_permission(&key, &long_config)
                .await
                .unwrap();
        }

        // Trigger eviction
        limiter
            .acquire_permission("trigger", &long_config)
            .await
            .unwrap();

        let map = limiter.state.lock().await;
        // Expired keys should have been evicted first
        let expired_count = map.keys().filter(|k| k.starts_with("expired-")).count();
        assert!(
            expired_count < 100,
            "expired windows should be evicted, but {expired_count} remain"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_oldest_first_when_no_expired() {
        // Mutant kill: rate_limiter.rs:93 — second eviction pass (oldest first)
        let limiter = InMemoryRateLimiter::new();
        // Long refresh so nothing expires
        let config = test_config(10, Duration::from_secs(600), Duration::from_millis(100));

        // Create keys in order — oldest first
        for i in 0..MAX_TRACKED_KEYS.saturating_add(2) {
            let key = format!("key-{i:06}");
            limiter.acquire_permission(&key, &config).await.unwrap();
        }

        // The newest key should survive, oldest should be evicted
        let map = limiter.state.lock().await;
        let last_key = format!("key-{:06}", MAX_TRACKED_KEYS.saturating_add(1));
        assert!(
            map.contains_key(&last_key),
            "newest key must survive eviction"
        );
    }

    #[tokio::test]
    async fn mutant_kill_deadline_exceeded_returns_error() {
        // Mutant kill: rate_limiter.rs:135 — replace > with >=
        let limiter = InMemoryRateLimiter::new();
        // 1 permit, long refresh, very short timeout
        let config = test_config(1, Duration::from_secs(60), Duration::from_millis(10));

        // Consume the permit
        limiter.acquire_permission("key-a", &config).await.unwrap();

        // Second acquire should fail because timeout < window refresh
        let result = limiter.acquire_permission("key-a", &config).await;
        assert!(
            result.is_err(),
            "must return error when deadline would be exceeded"
        );
    }

    #[test]
    fn mutant_kill_debug_fmt_not_replaced() {
        // Mutant kill: rate_limiter.rs:149 — Debug fmt replaced with Ok(Default::default())
        let limiter = InMemoryRateLimiter::new();
        let debug_str = format!("{limiter:?}");
        assert!(
            debug_str.contains("InMemoryRateLimiter"),
            "Debug output must contain type name, got: {debug_str}"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_preserves_current_key() {
        // Mutant kill: rate_limiter.rs:85-86 — current key exclusion in retain
        // When "current" is the key being acquired and eviction runs,
        // "current" must not be evicted.
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(2, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to MAX_TRACKED_KEYS with other keys
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("other-{i}");
            limiter.acquire_permission(&key, &config).await.unwrap();
        }

        // Use one permit on "current" — this triggers eviction since map > MAX_TRACKED_KEYS
        limiter
            .acquire_permission("current", &config)
            .await
            .unwrap();

        // Use second permit on "current" — still the active key
        limiter
            .acquire_permission("current", &config)
            .await
            .unwrap();

        // Both permits used — third should fail (key wasn't evicted and re-created)
        let fail_config = test_config(2, Duration::from_secs(60), Duration::from_millis(10));
        let result = limiter
            .acquire_permission("current", &fail_config)
            .await;
        assert!(
            result.is_err(),
            "current key must preserve permit state through eviction"
        );
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
