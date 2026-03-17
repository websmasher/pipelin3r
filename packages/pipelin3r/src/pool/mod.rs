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
    let mut results: Vec<_> = (0..total)
        .map(|_| Option::<Result<(), PipelineError>>::None)
        .collect();
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
        .map(|opt| {
            opt.unwrap_or_else(|| Err(PipelineError::Other(String::from("task result missing"))))
        })
        .collect()
}

/// Execute an async function for each item with bounded concurrency, returning
/// items paired with results.
///
/// The closure receives `(item, index, total)` and must return `(item, result)`.
/// The item goes IN to the closure and comes BACK OUT paired with the result,
/// solving the ownership problem without requiring `Clone`.
///
/// The `total` parameter is passed through to the closure and may differ from
/// `items.len()` — e.g. when the caller filtered a larger set and wants
/// progress to reflect the original count.
///
/// Results are returned in the original index order.
///
/// Individual task panics or cancellations are reported as
/// [`PipelineError::Other`] in the corresponding result slot. Because a
/// panicked task cannot return its `T`, those slots are omitted from the
/// output and the length may be shorter than the input.
pub async fn run_pool_map<T, F, Fut, R>(
    items: Vec<T>,
    concurrency: usize,
    total: usize,
    f: F,
) -> Vec<(T, Result<R, PipelineError>)>
where
    T: Send + 'static,
    F: Fn(T, usize, usize) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = (T, Result<R, PipelineError>)> + Send,
    R: Send + 'static,
{
    let effective_concurrency = if concurrency == 0 { 1 } else { concurrency };
    let semaphore = Arc::new(Semaphore::new(effective_concurrency));
    let count = items.len();

    // Pre-allocate result slots (indexed by original position for ordering)
    #[allow(
        clippy::type_complexity,
        reason = "pool result collection: type is internal"
    )]
    let mut results: Vec<Option<(T, Result<R, PipelineError>)>> =
        (0..count).map(|_| None).collect();
    let mut join_set = JoinSet::new();

    for (index, item) in items.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let func = f.clone();

        let _: tokio::task::AbortHandle = join_set.spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|e| PipelineError::Other(format!("semaphore closed: {e}")))?;
            let (item_back, result) = func(item, index, total).await;
            Ok::<(usize, T, Result<R, PipelineError>), PipelineError>((index, item_back, result))
        });
    }

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((index, item_back, task_result))) => {
                if let Some(slot) = results.get_mut(index) {
                    *slot = Some((item_back, task_result));
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Pool map task semaphore error: {e}");
            }
            Err(join_error) => {
                tracing::error!("Pool map task join error: {join_error}");
            }
        }
    }

    results.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests;
