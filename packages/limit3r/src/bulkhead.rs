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

        let Some(entry) = map.get_mut(key) else {
            // Defensive: we just inserted, this should never happen
            drop(map);
            return Arc::new(Semaphore::new(permits));
        };

        // If config changed, replace the semaphore
        if entry.max_concurrent != config.max_concurrent {
            entry.semaphore = Arc::new(Semaphore::new(permits));
            entry.max_concurrent = config.max_concurrent;
        }

        let result = Arc::clone(&entry.semaphore);
        drop(map);
        result
    }
}

impl Bulkhead for InMemoryBulkhead {
    async fn acquire(&self, key: &str, config: &BulkheadConfig) -> Result<(), Limit3rError> {
        // Evict keys with no outstanding permits when the map exceeds the size limit.
        {
            let map = self.state.read();
            if map.len() > MAX_TRACKED_KEYS {
                drop(map);
                let mut map = self.state.write();
                map.retain(|_k, entry| {
                    entry.semaphore.available_permits()
                        < usize::try_from(entry.max_concurrent).unwrap_or(usize::MAX)
                });
            }
        }

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
