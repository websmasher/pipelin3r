#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]
#![allow(
    clippy::significant_drop_tightening,
    reason = "test code: lock scope is intentional"
)]

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
        let exists = limiter.state.lock().await.contains_key(&format!("key-{i}"));
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
    limiter.acquire_permission("key-5", &config).await.unwrap();

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
    let result = limiter.acquire_permission("current", &fail_config).await;
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
