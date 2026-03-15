//! Semaphore-based concurrency limiter backed by in-memory state.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::config::BulkheadConfig;
use crate::error::Limit3rError;
use crate::traits::Bulkhead;
use parking_lot::RwLock;
use tokio::sync::Semaphore;

/// Per-key bulkhead state.
#[derive(Debug)]
struct BulkheadState {
    /// Semaphore controlling concurrent access.
    semaphore: Arc<Semaphore>,
    /// Configured maximum concurrency (used to detect config changes).
    max_concurrent: u32,
}

/// Maximum number of keys tracked before idle entries are evicted.
const MAX_TRACKED_KEYS: usize = 10_000;

/// Type alias to reduce type complexity for the inner state map.
type StateMap = BTreeMap<String, BulkheadState>;

/// In-memory semaphore-based bulkhead for concurrency limiting.
///
/// Each key gets its own [`Semaphore`] with `max_concurrent` permits.
/// Callers acquire a permit before executing and release it when done.
/// If no permits are available, the caller waits up to `max_wait_duration`
/// before receiving a [`Limit3rError::BulkheadFull`] error.
#[derive(Debug)]
pub struct InMemoryBulkhead {
    state: RwLock<StateMap>,
}

impl Default for InMemoryBulkhead {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBulkhead {
    /// Create a new, empty bulkhead.
    pub const fn new() -> Self {
        Self {
            state: RwLock::new(BTreeMap::new()),
        }
    }

    /// Get or create the semaphore for a given key.
    fn get_or_create_semaphore(&self, key: &str, config: &BulkheadConfig) -> Arc<Semaphore> {
        // Fast path: read lock
        let cached = {
            let map = self.state.read();
            map.get(key).and_then(|entry| {
                if entry.max_concurrent == config.max_concurrent {
                    Some(Arc::clone(&entry.semaphore))
                } else {
                    None
                }
            })
        };

        if let Some(sem) = cached {
            return sem;
        }

        // Slow path: write lock to insert or update
        let permits = usize::try_from(config.max_concurrent).unwrap_or(usize::MAX);
        let mut map = self.state.write();

        let needs_insert = !map.contains_key(key);
        if needs_insert {
            let _prev = map.insert(
                key.to_owned(),
                BulkheadState {
                    semaphore: Arc::new(Semaphore::new(permits)),
                    max_concurrent: config.max_concurrent,
                },
            );
        }

        // Update config if it changed.
        if let Some(entry) = map.get_mut(key) {
            if entry.max_concurrent != config.max_concurrent {
                entry.semaphore = Arc::new(Semaphore::new(permits));
                entry.max_concurrent = config.max_concurrent;
            }
        }

        // Evict idle keys (all permits available) when the map exceeds the
        // size limit, but never evict the current key.
        if map.len() > MAX_TRACKED_KEYS {
            map.retain(|k, entry| {
                k == key
                    || entry.semaphore.available_permits()
                        < usize::try_from(entry.max_concurrent).unwrap_or(usize::MAX)
            });
        }

        let Some(entry) = map.get(key) else {
            // Defensive: we just inserted, this should never happen
            drop(map);
            return Arc::new(Semaphore::new(permits));
        };

        let result = Arc::clone(&entry.semaphore);
        drop(map);
        result
    }
}

impl Bulkhead for InMemoryBulkhead {
    async fn acquire(&self, key: &str, config: &BulkheadConfig) -> Result<(), Limit3rError> {
        let semaphore = self.get_or_create_semaphore(key, config);

        let result = tokio::time::timeout(config.max_wait_duration, semaphore.acquire()).await;

        match result {
            Ok(Ok(permit)) => {
                // Forget the permit so it is not auto-released on drop.
                // The caller must explicitly call `release()` later.
                permit.forget();
                Ok(())
            }
            Ok(Err(_closed)) => {
                // Semaphore was closed — treat as full
                Err(Limit3rError::BulkheadFull {
                    key: key.to_owned(),
                })
            }
            Err(_timeout) => Err(Limit3rError::BulkheadFull {
                key: key.to_owned(),
            }),
        }
    }

    fn release(&self, key: &str) {
        let map = self.state.read();
        if let Some(entry) = map.get(key) {
            entry.semaphore.add_permits(1);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)] // reason: test assertions
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_config(max_concurrent: u32, max_wait: Duration) -> BulkheadConfig {
        BulkheadConfig {
            max_concurrent,
            max_wait_duration: max_wait,
        }
    }

    #[tokio::test]
    async fn acquires_permit_when_under_max_concurrent() {
        let bh = InMemoryBulkhead::new();
        let config = test_config(2, Duration::from_millis(100));

        let result = bh.acquire("key-a", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn releases_permit_correctly() {
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(50));

        // Acquire the single permit
        bh.acquire("key-a", &config).await.unwrap();

        // Release it
        bh.release("key-a");

        // Should be able to acquire again
        let result = bh.acquire("key-a", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn regression_bulkhead_eviction_does_not_evict_current_key() {
        // Regression: eviction could evict the key being acquired, causing
        // the semaphore to be re-created with fresh permits. After the fix,
        // the current key is always inserted before eviction runs.
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(100));

        // Fill to MAX_TRACKED_KEYS with idle keys (all permits available).
        for i in 0..MAX_TRACKED_KEYS.saturating_add(1) {
            let key = format!("filler-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        // Acquire on target key (consumes its single permit)
        bh.acquire("target-key", &config).await.unwrap();

        // Now trigger eviction by acquiring on another key
        bh.acquire("trigger-eviction", &config).await.unwrap();

        // The target key's permit should still be consumed.
        // If it was evicted and re-created, a second acquire would succeed.
        let result = bh
            .acquire("target-key", &test_config(1, Duration::from_millis(50)))
            .await;
        assert!(
            result.is_err(),
            "target key was evicted and re-created with fresh permits"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_triggers_at_max_tracked_keys() {
        // Mutant kill: bulkhead.rs:95 — replace > with ==/</>=/>=
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(100));

        // Fill to exactly MAX_TRACKED_KEYS (no eviction should happen yet)
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("fill-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        // Verify map is at MAX_TRACKED_KEYS
        let size_before = bh.state.read().len();
        assert_eq!(
            size_before, MAX_TRACKED_KEYS,
            "map should be at exactly MAX_TRACKED_KEYS"
        );

        // Add one more — should trigger eviction (map.len() > MAX_TRACKED_KEYS)
        bh.acquire("trigger", &config).await.unwrap();
        bh.release("trigger");

        let size_after = bh.state.read().len();
        assert!(
            size_after <= MAX_TRACKED_KEYS,
            "eviction should have reduced map size to at most MAX_TRACKED_KEYS, got {size_after}"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_does_not_trigger_below_max() {
        // Mutant kill: bulkhead.rs:95 — replace > with < or <=
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(100));

        // Fill to MAX_TRACKED_KEYS - 1 (below threshold)
        let below_max = MAX_TRACKED_KEYS.saturating_sub(1);
        for i in 0..below_max {
            let key = format!("fill-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        let size = bh.state.read().len();
        assert_eq!(
            size, below_max,
            "no eviction should happen below MAX_TRACKED_KEYS"
        );
    }

    #[tokio::test]
    async fn mutant_kill_config_change_replaces_semaphore() {
        // Mutant kill: bulkhead.rs:87 — replace != with == (config change detection)
        let bh = InMemoryBulkhead::new();
        let config2 = test_config(2, Duration::from_millis(100));
        let config5 = test_config(5, Duration::from_millis(100));

        // Create key with max_concurrent=2
        bh.acquire("key-x", &config2).await.unwrap();
        bh.release("key-x");

        // Acquire 3 permits with max_concurrent=5 — should succeed if config was updated
        bh.acquire("key-x", &config5).await.unwrap();
        bh.acquire("key-x", &config5).await.unwrap();
        bh.acquire("key-x", &config5).await.unwrap();

        // If config change was NOT detected (mutant: != replaced with ==),
        // the old semaphore with 2 permits would still be in use and the 3rd acquire
        // would have succeeded only because we released one earlier. Let's verify
        // we can get 4th and 5th permits too.
        bh.acquire("key-x", &config5).await.unwrap();
        let fifth = bh
            .acquire("key-x", &test_config(5, Duration::from_millis(50)))
            .await;
        assert!(
            fifth.is_ok(),
            "config change to max_concurrent=5 must allow 5 concurrent permits"
        );
    }

    #[tokio::test]
    async fn mutant_kill_config_match_returns_cached_semaphore() {
        // Mutant kill: bulkhead.rs:58 — replace == with != (config match check)
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(100));

        // Acquire the single permit
        bh.acquire("key-a", &config).await.unwrap();

        // Second acquire with SAME config should see the existing semaphore (0 permits left)
        let result = bh
            .acquire("key-a", &test_config(1, Duration::from_millis(50)))
            .await;
        assert!(
            result.is_err(),
            "same config must reuse cached semaphore (no permits left)"
        );
    }

    #[tokio::test]
    async fn mutant_kill_eviction_keeps_keys_with_outstanding_permits() {
        // Mutant kill: bulkhead.rs:98-99 — replace || with && and < with other comparisons
        let bh = InMemoryBulkhead::new();
        let config = test_config(2, Duration::from_millis(100));

        // Create a key with an outstanding permit (not all permits available)
        bh.acquire("busy-key", &config).await.unwrap();
        // busy-key has 1 of 2 permits used — available_permits=1 < max_concurrent=2

        // Fill to MAX_TRACKED_KEYS with idle keys (all permits released)
        for i in 0..MAX_TRACKED_KEYS {
            let key = format!("idle-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        // Trigger eviction
        bh.acquire("trigger-evict", &config).await.unwrap();
        bh.release("trigger-evict");

        // busy-key must survive eviction because it has outstanding permits
        let map = bh.state.read();
        assert!(
            map.contains_key("busy-key"),
            "key with outstanding permits must survive eviction"
        );
    }

    #[tokio::test]
    async fn times_out_when_all_permits_taken() {
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(50));

        // Acquire the single permit (don't release)
        bh.acquire("key-a", &config).await.unwrap();

        // Second acquire should time out
        let result = bh.acquire("key-a", &config).await;
        assert!(result.is_err());
    }
}
