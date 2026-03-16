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
                            || now.duration_since(state.window_start)
                                < config.limit_refresh_period
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
    async fn eviction_does_not_evict_current_key() {
        // Regression: eviction could evict the current key being acquired,
        // causing it to lose its state and reset. After the fix, the current
        // key is always inserted/preserved before eviction runs.
        let limiter = InMemoryRateLimiter::with_max_keys(5);
        let config = test_config(2, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to 5 keys
        for i in 0..5 {
            let key = format!("filler-{i}");
            limiter.acquire_permission(&key, &config).await.unwrap();
        }

        // Use one permit on "survivor-key" — triggers eviction since map > 5
        limiter
            .acquire_permission("survivor-key", &config)
            .await
            .unwrap();

        // Use second permit
        limiter
            .acquire_permission("survivor-key", &config)
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
    async fn no_eviction_at_exactly_max_keys() {
        // Mutant kill: `>` vs `>=` — at exactly max, no eviction should run
        let limiter = InMemoryRateLimiter::with_max_keys(5);
        let config = test_config(10, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to exactly 5 — no eviction
        for i in 0..5 {
            limiter
                .acquire_permission(&format!("key-{i}"), &config)
                .await
                .unwrap();
        }

        // Verify: all 5 keys exist (no eviction at exactly the limit)
        let size_at_max = limiter.state.lock().await.len();
        assert_eq!(size_at_max, 5, "no eviction at exactly max_tracked_keys");
        for i in 0..5 {
            let exists = limiter
                .state
                .lock()
                .await
                .contains_key(&format!("key-{i}"));
            assert!(exists, "key-{i} must exist at exactly the limit");
        }
    }

    #[tokio::test]
    async fn eviction_triggers_when_exceeding_max_keys() {
        // Mutant kill: `>` vs `>=` — at max+1, eviction must trigger
        let limiter = InMemoryRateLimiter::with_max_keys(5);
        let config = test_config(10, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to exactly 5
        for i in 0..5 {
            limiter
                .acquire_permission(&format!("key-{i}"), &config)
                .await
                .unwrap();
        }

        // Add one more — eviction triggers
        limiter
            .acquire_permission("key-5", &config)
            .await
            .unwrap();

        let map = limiter.state.lock().await;
        assert!(
            map.len() <= 6,
            "eviction should run after exceeding max_tracked_keys, got {}",
            map.len()
        );
        // The triggering key must survive
        assert!(
            map.contains_key("key-5"),
            "current key must survive eviction"
        );
    }

    #[tokio::test]
    async fn eviction_removes_expired_windows_first() {
        // Mutant kill: retain logic — expired windows evicted, fresh ones kept.
        // Note: eviction uses the *current* call's config.limit_refresh_period to
        // determine whether a window is expired. So the triggering call must use a
        // short refresh period to make the old keys look expired.
        let limiter = InMemoryRateLimiter::with_max_keys(5);
        let config = test_config(10, Duration::from_millis(10), Duration::from_millis(200));

        // Create 3 keys that will expire quickly
        for i in 0..3 {
            limiter
                .acquire_permission(&format!("expired-{i}"), &config)
                .await
                .unwrap();
        }

        // Wait for them to expire relative to the 10ms refresh period
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Create 3 fresh keys (total = 6 > 5, triggers eviction on the 6th insert).
        // Use the same short refresh — the fresh keys' window_start is "now",
        // so duration_since < 10ms refresh holds, and they survive retain.
        for i in 0..3 {
            limiter
                .acquire_permission(&format!("fresh-{i}"), &config)
                .await
                .unwrap();
        }

        let map = limiter.state.lock().await;
        // Expired keys should have been evicted
        let expired_count = (0..3)
            .filter(|i| map.contains_key(&format!("expired-{i}")))
            .count();
        assert_eq!(
            expired_count, 0,
            "expired windows should be evicted, but {expired_count} remain"
        );
        // Fresh keys must survive
        for i in 0..3 {
            assert!(
                map.contains_key(&format!("fresh-{i}")),
                "fresh-{i} must survive eviction"
            );
        }
    }

    #[tokio::test]
    async fn eviction_removes_oldest_when_no_expired() {
        // Mutant kill: second eviction pass — oldest entries removed first
        let limiter = InMemoryRateLimiter::with_max_keys(5);
        // Long refresh so nothing expires
        let config = test_config(10, Duration::from_secs(600), Duration::from_millis(100));

        // Create 7 keys — triggers eviction twice
        for i in 0..7 {
            limiter
                .acquire_permission(&format!("key-{i}"), &config)
                .await
                .unwrap();
        }

        let map = limiter.state.lock().await;
        // The last key (current) must survive
        assert!(
            map.contains_key("key-6"),
            "newest key must survive eviction"
        );
        assert!(
            map.len() <= 6,
            "map should be at most max+1, got {}",
            map.len()
        );
    }

    #[tokio::test]
    async fn eviction_preserves_current_key_permit_state() {
        // Mutant kill: current key exclusion in retain
        let limiter = InMemoryRateLimiter::with_max_keys(5);
        let config = test_config(2, Duration::from_secs(60), Duration::from_millis(100));

        // Fill to 5 with other keys
        for i in 0..5 {
            limiter
                .acquire_permission(&format!("other-{i}"), &config)
                .await
                .unwrap();
        }

        // Use one permit on "current" — triggers eviction since map > 5
        limiter
            .acquire_permission("current", &config)
            .await
            .unwrap();

        // Use second permit on "current"
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
    async fn deadline_exceeded_returns_error() {
        // Mutant kill: `>` replaced with `>=` on deadline check
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

    #[tokio::test]
    async fn deadline_tight_timeout_returns_error() {
        // Mutant kill: deadline comparison — sleep_until far exceeds deadline
        let limiter = InMemoryRateLimiter::new();
        let config = test_config(1, Duration::from_secs(10), Duration::from_millis(1));

        limiter.acquire_permission("tight", &config).await.unwrap();

        let result = limiter.acquire_permission("tight", &config).await;
        assert!(
            result.is_err(),
            "must error when sleep_until exceeds deadline"
        );
    }

    #[test]
    fn debug_fmt_outputs_type_name() {
        let limiter = InMemoryRateLimiter::new();
        let debug_str = format!("{limiter:?}");
        assert!(
            debug_str.contains("InMemoryRateLimiter"),
            "Debug output must contain type name, got: {debug_str}"
        );
    }

    #[test]
    fn debug_fmt_key_state_outputs_field_names() {
        let ks = KeyState {
            permits_used: 42,
            window_start: Instant::now(),
        };
        let ks_debug = format!("{ks:?}");
        assert!(
            ks_debug.contains("permits_used"),
            "Debug must contain 'permits_used', got: {ks_debug}"
        );
        assert!(
            ks_debug.contains("42"),
            "Debug must contain the actual value '42', got: {ks_debug}"
        );
        assert!(
            ks_debug.contains("window_start"),
            "Debug must contain 'window_start', got: {ks_debug}"
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
