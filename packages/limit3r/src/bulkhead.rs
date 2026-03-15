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
    /// Maximum number of keys tracked before idle entries are evicted.
    max_tracked_keys: usize,
}

impl Default for InMemoryBulkhead {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBulkhead {
    /// Create a new, empty bulkhead with the default key limit (10,000).
    pub const fn new() -> Self {
        Self::with_max_keys(MAX_TRACKED_KEYS)
    }

    /// Create a new, empty bulkhead with a custom maximum number of tracked keys.
    pub const fn with_max_keys(max: usize) -> Self {
        Self {
            state: RwLock::new(BTreeMap::new()),
            max_tracked_keys: max,
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
        if map.len() > self.max_tracked_keys {
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
    async fn eviction_does_not_evict_current_key_bulkhead() {
        // Regression: eviction could evict the key being acquired
        let bh = InMemoryBulkhead::with_max_keys(5);
        let config = test_config(1, Duration::from_millis(100));

        // Fill to 5 with idle keys (all permits available)
        for i in 0..5 {
            let key = format!("filler-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        // Acquire on target key (consumes its single permit)
        bh.acquire("target-key", &config).await.unwrap();

        // Now trigger eviction by acquiring on another key
        bh.acquire("trigger-eviction", &config).await.unwrap();

        // The target key's permit should still be consumed
        let result = bh
            .acquire("target-key", &test_config(1, Duration::from_millis(50)))
            .await;
        assert!(
            result.is_err(),
            "target key was evicted and re-created with fresh permits"
        );
    }

    #[tokio::test]
    async fn no_eviction_at_exactly_max_keys_bulkhead() {
        // Mutant kill: `>` vs `>=` — at exactly max, no eviction should run
        let bh = InMemoryBulkhead::with_max_keys(5);
        let config = test_config(1, Duration::from_millis(100));

        // Fill to exactly 5 (no eviction should happen yet)
        for i in 0..5 {
            let key = format!("fill-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        // All 5 keys must exist
        let size_at_max = bh.state.read().len();
        assert_eq!(size_at_max, 5, "no eviction at exactly max_tracked_keys");
        for i in 0..5 {
            let exists = bh.state.read().contains_key(&format!("fill-{i}"));
            assert!(exists, "fill-{i} must exist at exactly the limit");
        }
    }

    #[tokio::test]
    async fn eviction_triggers_when_exceeding_max_keys_bulkhead() {
        // Mutant kill: `>` vs `>=` — at max+1, eviction must trigger
        let bh = InMemoryBulkhead::with_max_keys(5);
        let config = test_config(1, Duration::from_millis(100));

        // Fill to exactly 5
        for i in 0..5 {
            let key = format!("fill-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        // Add one more — should trigger eviction
        bh.acquire("trigger", &config).await.unwrap();
        bh.release("trigger");

        let size_after = bh.state.read().len();
        assert!(
            size_after <= 5,
            "eviction should have reduced map size to at most max_tracked_keys, got {size_after}"
        );
    }

    #[tokio::test]
    async fn no_eviction_below_max_keys_bulkhead() {
        let bh = InMemoryBulkhead::with_max_keys(5);
        let config = test_config(1, Duration::from_millis(100));

        // Fill to 4 (below threshold)
        for i in 0..4 {
            let key = format!("fill-{i}");
            bh.acquire(&key, &config).await.unwrap();
            bh.release(&key);
        }

        let size = bh.state.read().len();
        assert_eq!(size, 4, "no eviction should happen below max_tracked_keys");
    }

    #[tokio::test]
    async fn config_change_replaces_semaphore() {
        let bh = InMemoryBulkhead::new();
        let config2 = test_config(2, Duration::from_millis(100));
        let config5 = test_config(5, Duration::from_millis(100));

        bh.acquire("key-x", &config2).await.unwrap();
        bh.release("key-x");

        // Acquire 5 permits with max_concurrent=5
        for _ in 0..5 {
            bh.acquire("key-x", &config5).await.unwrap();
        }

        // 6th should fail (all 5 used)
        let sixth = bh
            .acquire("key-x", &test_config(5, Duration::from_millis(50)))
            .await;
        assert!(
            sixth.is_err(),
            "config change to max_concurrent=5 should allow exactly 5 permits, not more"
        );
    }

    #[tokio::test]
    async fn config_match_returns_cached_semaphore() {
        let bh = InMemoryBulkhead::new();
        let config = test_config(1, Duration::from_millis(100));

        bh.acquire("key-a", &config).await.unwrap();

        let result = bh
            .acquire("key-a", &test_config(1, Duration::from_millis(50)))
            .await;
        assert!(
            result.is_err(),
            "same config must reuse cached semaphore (no permits left)"
        );
    }

    #[tokio::test]
    async fn eviction_keeps_keys_with_outstanding_permits() {
        let bh = InMemoryBulkhead::with_max_keys(5);
        let config = test_config(2, Duration::from_millis(100));

        // Create a key with an outstanding permit
        bh.acquire("busy-key", &config).await.unwrap();

        // Fill with idle keys (all permits released)
        for i in 0..5 {
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

        bh.acquire("key-a", &config).await.unwrap();

        let result = bh.acquire("key-a", &config).await;
        assert!(result.is_err());
    }
}
