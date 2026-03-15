//! Bounded async concurrency pool.
//!
//! Runs async tasks with a semaphore-based concurrency limit, starting the next
//! task as each completes. Returns one result per item so the caller can inspect
//! which items failed.

use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::error::PipelineError;

/// Run async tasks with bounded concurrency.
///
/// Fires at most `concurrency` tasks at a time, starting the next as each completes.
/// Returns a `Vec` of results (one per item) so the caller can inspect which items failed.
///
/// Individual task errors are captured in the returned `Vec`. Spawned task panics
/// or cancellations are reported as errors in their respective slots.
pub async fn run_pool<T, F, Fut>(
    items: Vec<T>,
    concurrency: usize,
    f: F,
) -> Vec<Result<(), PipelineError>>
where
    T: Send + 'static,
    F: Fn(T, usize) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = Result<(), PipelineError>> + Send,
{
    let effective_concurrency = if concurrency == 0 { 1 } else { concurrency };
    let semaphore = Arc::new(Semaphore::new(effective_concurrency));
    let total = items.len();
    let mut results: Vec<Option<Result<(), PipelineError>>> = (0..total).map(|_| None).collect();
    let mut join_set = JoinSet::new();

    for (index, item) in items.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let func = f.clone();

        let _: tokio::task::AbortHandle = join_set.spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|e| PipelineError::Other(format!("semaphore closed: {e}")))?;
            let result = func(item, index).await;
            Ok::<(usize, Result<(), PipelineError>), PipelineError>((index, result))
        });
    }

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((index, task_result))) => {
                if let Some(slot) = results.get_mut(index) {
                    *slot = Some(task_result);
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Pool task semaphore error: {e}");
            }
            Err(join_error) => {
                tracing::error!("Pool task join error: {join_error}");
            }
        }
    }

    results
        .into_iter()
        .map(|opt| opt.unwrap_or_else(|| Err(PipelineError::Other(String::from("task result missing")))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    #[allow(clippy::unwrap_used)] // reason: test assertion
    async fn runs_all_items() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let items: Vec<usize> = (0..5).collect();
        let results = run_pool(items, 2, move |_item, _index| {
            let c = Arc::clone(&counter_clone);
            async move {
                let _ = c.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await;

        assert_eq!(results.len(), 5, "should have one result per item");
        assert_eq!(counter.load(Ordering::Relaxed), 5, "all items processed");
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok(), "item {i} should succeed");
        }
    }

    #[tokio::test]
    async fn captures_individual_failures() {
        let items: Vec<usize> = (0..3).collect();
        let results = run_pool(items, 2, |item, _index| async move {
            if item == 1 {
                return Err(PipelineError::Other(String::from("item 1 failed")));
            }
            Ok(())
        })
        .await;

        assert_eq!(results.len(), 3, "should have one result per item");

        let mut successes: usize = 0;
        let mut failures: usize = 0;
        for r in &results {
            if r.is_ok() {
                successes = successes.saturating_add(1);
            } else {
                failures = failures.saturating_add(1);
            }
        }
        assert_eq!(successes, 2, "two items should succeed");
        assert_eq!(failures, 1, "one item should fail");
    }

    #[tokio::test]
    async fn mutant_kill_pool_zero_concurrency_runs_all() {
        // Mutant kill: pool.rs:31 — `== with !=` on `if concurrency == 0 { 1 }`
        // With concurrency=0, the pool must still run all items (effective concurrency=1).
        // If mutated to !=, concurrency=0 would not be corrected to 1, causing
        // Semaphore::new(0) which deadlocks.
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let items: Vec<usize> = (0..3).collect();
        let results = run_pool(items, 0, move |_item, _index| {
            let c = Arc::clone(&counter_clone);
            async move {
                let _ = c.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await;

        assert_eq!(results.len(), 3, "should have one result per item");
        assert_eq!(
            counter.load(Ordering::Relaxed),
            3,
            "all 3 items must be processed even with concurrency=0"
        );
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok(), "item {i} should succeed");
        }
    }

    #[tokio::test]
    async fn mutant_kill_pool_empty_items_returns_empty() {
        // Mutant kill: pool.rs:31 — `== with !=` also means concurrency=2 (nonzero)
        // would be wrongly set to 1 via the mutant. But more importantly, empty vec
        // must return empty results regardless.
        let items: Vec<usize> = vec![];
        let results = run_pool(items, 0, |_item, _index| async { Ok(()) }).await;
        assert!(
            results.is_empty(),
            "empty input with concurrency=0 must produce empty output"
        );
    }

    #[tokio::test]
    async fn empty_items_returns_empty() {
        let items: Vec<usize> = vec![];
        let results = run_pool(items, 2, |_item, _index| async { Ok(()) }).await;
        assert!(
            results.is_empty(),
            "empty input should produce empty output"
        );
    }
}
