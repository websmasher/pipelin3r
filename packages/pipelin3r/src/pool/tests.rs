#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
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

// ── run_pool_map tests ──

#[tokio::test]
async fn pool_map_returns_items_with_results() {
    let items: Vec<String> = vec![
        String::from("alpha"),
        String::from("beta"),
        String::from("gamma"),
    ];
    let results = run_pool_map(items, 2, 3, |item, _idx, _total| async move {
        let len = item.len();
        (item, Ok(len))
    })
    .await;

    assert_eq!(results.len(), 3, "should have one result per item");
    for (item, result) in &results {
        let expected_len = item.len();
        assert_eq!(
            result.as_ref().ok(),
            Some(&expected_len),
            "result should match item length"
        );
    }
}

#[tokio::test]
async fn pool_map_preserves_order() {
    let items: Vec<usize> = (0..10).collect();
    let results = run_pool_map(items, 3, 10, |item, _idx, _total| async move {
        // Add a small delay proportional to item to mix up completion order
        if item % 2 == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        (item, Ok(item))
    })
    .await;

    assert_eq!(results.len(), 10);
    for (i, (item, result)) in results.iter().enumerate() {
        assert_eq!(*item, i, "items should be in original order");
        assert_eq!(result.as_ref().ok(), Some(&i));
    }
}

#[tokio::test]
async fn pool_map_concurrency_bounded() {
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let active_clone = Arc::clone(&active);
    let max_active_clone = Arc::clone(&max_active);

    let items: Vec<usize> = (0..10).collect();
    let _results = run_pool_map(items, 2, 10, move |item, _idx, _total| {
        let a = Arc::clone(&active_clone);
        let m = Arc::clone(&max_active_clone);
        async move {
            let prev = a.fetch_add(1, Ordering::SeqCst);
            let current = prev.saturating_add(1);
            // Update max seen concurrency
            loop {
                let old_max = m.load(Ordering::SeqCst);
                if current <= old_max {
                    break;
                }
                if m.compare_exchange(old_max, current, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = a.fetch_sub(1, Ordering::SeqCst);
            (item, Ok(()))
        }
    })
    .await;

    let observed_max = max_active.load(Ordering::SeqCst);
    assert!(
        observed_max <= 2,
        "max concurrent should be <= 2, got {observed_max}"
    );
}

#[tokio::test]
async fn pool_map_empty_input() {
    let items: Vec<usize> = vec![];
    let results = run_pool_map(items, 2, 0, |item, _idx, _total| async move {
        (item, Ok::<(), PipelineError>(()))
    })
    .await;
    assert!(
        results.is_empty(),
        "empty input should produce empty output"
    );
}

#[tokio::test]
async fn pool_map_single_item() {
    let items = vec![42_usize];
    let results = run_pool_map(items, 4, 1, |item, idx, total| async move {
        assert_eq!(idx, 0);
        assert_eq!(total, 1);
        (item, Ok(String::from("done")))
    })
    .await;

    assert_eq!(results.len(), 1);
    let (item, result) = results.into_iter().next().unwrap();
    assert_eq!(item, 42);
    assert_eq!(result.unwrap(), "done");
}

#[tokio::test]
async fn pool_map_total_differs_from_len() {
    // total=100 but only 3 items — the closure should see total=100
    let items: Vec<usize> = vec![0, 1, 2];
    let results = run_pool_map(items, 2, 100, |item, _idx, total| async move {
        (item, Ok(total))
    })
    .await;

    assert_eq!(results.len(), 3);
    for (_, result) in &results {
        assert_eq!(result.as_ref().ok(), Some(&100_usize));
    }
}

#[tokio::test]
async fn pool_map_zero_concurrency() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = Arc::clone(&counter);

    let items: Vec<usize> = (0..3).collect();
    let results = run_pool_map(items, 0, 3, move |item, _idx, _total| {
        let c = Arc::clone(&counter_clone);
        async move {
            let _ = c.fetch_add(1, Ordering::Relaxed);
            (item, Ok(()))
        }
    })
    .await;

    assert_eq!(results.len(), 3);
    assert_eq!(counter.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn pool_map_partial_failure() {
    let items: Vec<usize> = (0..4).collect();
    let results = run_pool_map(items, 2, 4, |item, _idx, _total| async move {
        if item == 2 {
            (
                item,
                Err(PipelineError::Other(String::from("item 2 failed"))),
            )
        } else {
            (item, Ok(String::from("ok")))
        }
    })
    .await;

    assert_eq!(results.len(), 4);
    let mut successes: usize = 0;
    let mut failures: usize = 0;
    for (_, r) in &results {
        if r.is_ok() {
            successes = successes.saturating_add(1);
        } else {
            failures = failures.saturating_add(1);
        }
    }
    assert_eq!(successes, 3);
    assert_eq!(failures, 1);
}
