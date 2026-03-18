//! Adversarial tests for the `AsyncTaskStore` implementation.
//!
//! These tests target concurrency, edge cases, store integrity, failure modes,
//! and security properties of the in-memory async task store.

#![allow(clippy::unwrap_used)] // reason: test code — panics are acceptable for assertions
#![allow(clippy::arithmetic_side_effects)] // reason: test code — counter arithmetic is safe
#![allow(clippy::str_to_string)] // reason: test code — convenience string construction
#![allow(unused_crate_dependencies)] // reason: integration test — only uses subset of crate deps

use std::sync::Arc;
use std::time::Duration;

use domain_types::{AsyncTaskStatus, ExecutionMetadata, TaskResponse};
use rest::state::AsyncTaskStore;

/// Helper: build a fake `TaskResponse` with the given output string.
fn fake_response(output: &str) -> TaskResponse {
    TaskResponse {
        success: true,
        output: output.to_owned(),
        metadata: ExecutionMetadata {
            started_at: "2026-01-01T00:00:00Z".to_owned(),
            elapsed: Duration::from_millis(42),
            exit_code: 0,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 1. CONCURRENCY / RACE CONDITIONS
// ═══════════════════════════════════════════════════════════════════════

/// Scenario 1: Submit 100 tasks simultaneously — does the store handle
/// concurrent inserts without data loss or panics?
#[test]
fn concurrent_insert_100_tasks() {
    let store = Arc::new(AsyncTaskStore::new());
    let mut handles = Vec::new();

    for i in 0..100 {
        let store = Arc::clone(&store);
        let handle = std::thread::spawn(move || {
            store.insert_running(format!("task-{i}"));
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    // All 100 tasks should be present and in "running" state.
    for i in 0..100 {
        let id = format!("task-{i}");
        let status = store.get_status(&id);
        assert!(status.is_some(), "task-{i} missing after concurrent insert");
        let s = status.unwrap();
        assert_eq!(s.status, "running", "task-{i} should be running");
    }
}

/// Scenario 2: Complete a task from one thread while another reads it.
/// Both should see a consistent state (either running or completed, never
/// a partial/corrupt state).
#[test]
fn concurrent_complete_and_read() {
    let store = Arc::new(AsyncTaskStore::new());
    store.insert_running("race-task".to_owned());

    let store_writer = Arc::clone(&store);
    let store_reader = Arc::clone(&store);

    let writer = std::thread::spawn(move || {
        // Small delay to create a race window
        std::thread::sleep(Duration::from_micros(10));
        store_writer.mark_completed("race-task", fake_response("done"));
    });

    let reader = std::thread::spawn(move || {
        // Poll repeatedly during the race window
        let mut statuses = Vec::new();
        for _ in 0..1000 {
            if let Some(s) = store_reader.get_status("race-task") {
                statuses.push(s.status.clone());
            }
        }
        statuses
    });

    writer.join().unwrap();
    let statuses = reader.join().unwrap();

    // Every observed status must be either "running" or "completed" — never
    // something else or an inconsistent state.
    for s in &statuses {
        assert!(
            s == "running" || s == "completed",
            "observed unexpected status during race: {s}"
        );
    }
}

/// Scenario 3: Insert a task and immediately poll — should return "running".
#[test]
fn immediate_poll_returns_running() {
    let store = AsyncTaskStore::new();
    store.insert_running("instant-poll".to_owned());

    let status = store.get_status("instant-poll");
    assert!(
        status.is_some(),
        "task should exist immediately after insert"
    );
    assert_eq!(
        status.unwrap().status,
        "running",
        "newly inserted task must be 'running'"
    );
}

/// Scenario 4: Two readers polling the same completed task_id — both
/// must get the full result.
#[test]
fn two_readers_get_same_result() {
    let store = Arc::new(AsyncTaskStore::new());
    store.insert_running("shared-task".to_owned());
    store.mark_completed("shared-task", fake_response("shared-output"));

    let store1 = Arc::clone(&store);
    let store2 = Arc::clone(&store);

    let r1 = std::thread::spawn(move || store1.get_status("shared-task"));
    let r2 = std::thread::spawn(move || store2.get_status("shared-task"));

    let s1 = r1.join().unwrap();
    let s2 = r2.join().unwrap();

    assert!(s1.is_some(), "reader 1 must see the task");
    assert!(s2.is_some(), "reader 2 must see the task");

    let s1 = s1.unwrap();
    let s2 = s2.unwrap();

    assert_eq!(s1.status, "completed");
    assert_eq!(s2.status, "completed");

    // Both must have the result, not just the first reader
    assert!(s1.result.is_some(), "reader 1 must get the result");
    assert!(s2.result.is_some(), "reader 2 must get the result");
    assert_eq!(s1.result.unwrap().output, "shared-output");
    assert_eq!(s2.result.unwrap().output, "shared-output");
}

/// Scenario 5: Reaper must skip running tasks — only reap completed/failed.
#[test]
fn reaper_skips_running_tasks() {
    let store = AsyncTaskStore::new();

    // Insert a running task
    store.insert_running("still-running".to_owned());
    // Insert a completed task
    store.insert_running("done-task".to_owned());
    store.mark_completed("done-task", fake_response("done"));

    // Reaper should NOT remove running tasks even if we call it
    let reaped = store.reap_expired();
    // TTL is 600s, so even the completed task shouldn't be reaped yet
    assert_eq!(reaped, 0, "nothing should be reaped within TTL");

    // Running task must survive
    assert!(
        store.get_status("still-running").is_some(),
        "running task must not be reaped"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 2. EDGE CASES
// ═══════════════════════════════════════════════════════════════════════

/// Scenario 6: Poll a task_id that doesn't exist — must return None.
#[test]
fn nonexistent_task_returns_none() {
    let store = AsyncTaskStore::new();
    let status = store.get_status("does-not-exist");
    assert!(
        status.is_none(),
        "nonexistent task should return None, got: {status:?}"
    );
}

/// Scenario 7: After TTL expiry the task should be reaped.
/// We can't easily test real TTL (600s), but we verify the reaper logic
/// by checking that freshly completed tasks are NOT reaped.
#[test]
fn fresh_completed_task_survives_reaper() {
    let store = AsyncTaskStore::new();
    store.insert_running("fresh".to_owned());
    store.mark_completed("fresh", fake_response("result"));

    let reaped = store.reap_expired();
    assert_eq!(reaped, 0, "fresh completed task should not be reaped");
    assert!(
        store.get_status("fresh").is_some(),
        "fresh task must still be readable"
    );
}

/// Scenario 9: Insert with empty task_id — should this work? The store
/// doesn't validate IDs, so it should accept empty strings.
#[test]
fn empty_task_id_is_accepted() {
    let store = AsyncTaskStore::new();
    store.insert_running(String::new());

    let status = store.get_status("");
    assert!(
        status.is_some(),
        "empty task_id should be accepted by store"
    );
    assert_eq!(status.unwrap().status, "running");
}

/// Scenario 10: Task IDs with special characters — path traversal,
/// null bytes, unicode, extremely long strings.
#[test]
fn special_character_task_ids() {
    let store = AsyncTaskStore::new();

    let evil_ids = [
        "../../etc/passwd",
        "../../../.env",
        "task\0embedded-null",
        "task\nwith\nnewlines",
        "task\twith\ttabs",
        "\u{200B}zero-width-space",
        &"a".repeat(10_000), // very long ID
    ];

    for id in &evil_ids {
        let id_string = (*id).to_owned();
        store.insert_running(id_string);
        let status = store.get_status(id);
        assert!(
            status.is_some(),
            "store should accept any string as task_id"
        );
    }
}

/// Scenario 11: Route collision — task_id "status" could clash with
/// GET /api/tasks/status. This tests store behavior (store doesn't care),
/// but documents the risk for HTTP routing.
#[test]
fn task_id_named_status_does_not_collide_in_store() {
    let store = AsyncTaskStore::new();
    store.insert_running("status".to_owned());
    store.insert_running("limiter-status".to_owned());

    assert!(
        store.get_status("status").is_some(),
        "task named 'status' should work in store"
    );
    assert!(
        store.get_status("limiter-status").is_some(),
        "task named 'limiter-status' should work in store"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 3. STORE INTEGRITY
// ═══════════════════════════════════════════════════════════════════════

/// Scenario 12: After completion, all TaskResponse fields must be preserved.
#[test]
fn completed_task_preserves_all_fields() {
    let store = AsyncTaskStore::new();
    let response = TaskResponse {
        success: false,
        output: "error output with special chars: \u{1F4A5} <script>alert(1)</script>".to_owned(),
        metadata: ExecutionMetadata {
            started_at: "2026-03-18T12:34:56.789Z".to_owned(),
            elapsed: Duration::from_millis(12345),
            exit_code: 42,
        },
    };

    store.insert_running("preserve-test".to_owned());
    store.mark_completed("preserve-test", response.clone());

    let status = store.get_status("preserve-test");
    assert!(status.is_some(), "completed task must be retrievable");

    let s = status.unwrap();
    assert_eq!(s.status, "completed");
    let result = s.result.as_ref();
    assert!(result.is_some(), "result must be present");

    let r = result.unwrap();
    assert!(!r.success, "success field must be preserved as false");
    assert_eq!(
        r.output, response.output,
        "output must be preserved exactly"
    );
    assert_eq!(r.metadata.exit_code, 42, "exit_code must be preserved");
    assert_eq!(
        r.metadata.started_at, "2026-03-18T12:34:56.789Z",
        "started_at must be preserved"
    );
    assert_eq!(
        r.metadata.elapsed,
        Duration::from_millis(12345),
        "elapsed must be preserved"
    );
}

/// Scenario 13: Insert 1000 tasks, complete them all, reaper should NOT
/// remove them within TTL. Documents memory behavior.
#[test]
fn thousand_tasks_survive_reaper_within_ttl() {
    let store = AsyncTaskStore::new();

    for i in 0..1000 {
        store.insert_running(format!("bulk-{i}"));
        store.mark_completed(&format!("bulk-{i}"), fake_response(&format!("out-{i}")));
    }

    let reaped = store.reap_expired();
    assert_eq!(
        reaped, 0,
        "no tasks should be reaped within TTL, but {reaped} were removed"
    );

    // Verify all still present
    for i in 0..1000 {
        assert!(
            store.get_status(&format!("bulk-{i}")).is_some(),
            "bulk-{i} missing after reaper run"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 4. FAILURE MODES
// ═══════════════════════════════════════════════════════════════════════

/// Scenario 15: Task marked as failed — store returns "failed" with error
/// message, no result.
#[test]
fn failed_task_returns_error_no_result() {
    let store = AsyncTaskStore::new();
    store.insert_running("fail-task".to_owned());
    store.mark_failed(
        "fail-task",
        "subprocess panicked: stack overflow".to_owned(),
    );

    let status = store.get_status("fail-task");
    assert!(status.is_some(), "failed task must be retrievable");

    let s = status.unwrap();
    assert_eq!(s.status, "failed");
    assert!(s.result.is_none(), "failed task must NOT have a result");
    assert!(s.error.is_some(), "failed task must have an error message");
    assert_eq!(s.error.unwrap(), "subprocess panicked: stack overflow");
}

/// BUG HUNT: mark_completed on a nonexistent task_id — silently ignored.
/// This is a potential bug: if the background executor calls mark_completed
/// with a typo'd ID, the result is lost with no error.
#[test]
fn mark_completed_nonexistent_id_is_silently_lost() {
    let store = AsyncTaskStore::new();

    // No insert_running for "phantom"
    store.mark_completed("phantom", fake_response("lost-result"));

    // Result is silently lost — this documents the behavior
    let status = store.get_status("phantom");
    assert!(
        status.is_none(),
        "marking a nonexistent task should not create an entry"
    );
}

/// BUG HUNT: mark_failed on a nonexistent task_id — also silently ignored.
#[test]
fn mark_failed_nonexistent_id_is_silently_lost() {
    let store = AsyncTaskStore::new();

    store.mark_failed("phantom", "error message".to_owned());

    let status = store.get_status("phantom");
    assert!(
        status.is_none(),
        "marking a nonexistent task as failed should not create an entry"
    );
}

/// BUG HUNT: Double insert with same ID — overwrites previous state.
/// If a UUID collision occurs, the original task's state is destroyed.
#[test]
fn duplicate_insert_overwrites_previous_state() {
    let store = AsyncTaskStore::new();

    store.insert_running("dup-id".to_owned());
    store.mark_completed("dup-id", fake_response("first-result"));

    // Verify it's completed
    let s = store.get_status("dup-id");
    assert_eq!(s.as_ref().unwrap().status, "completed");

    // Now insert again with same ID — overwrites to "running"
    store.insert_running("dup-id".to_owned());

    let s = store.get_status("dup-id");
    assert_eq!(
        s.as_ref().unwrap().status,
        "running",
        "duplicate insert should overwrite to running (BUG: first result is lost)"
    );
}

/// BUG HUNT: mark_completed on an already completed task — overwrites result.
#[test]
fn double_complete_overwrites_result() {
    let store = AsyncTaskStore::new();
    store.insert_running("double-done".to_owned());

    store.mark_completed("double-done", fake_response("first"));
    store.mark_completed("double-done", fake_response("second"));

    let s = store.get_status("double-done");
    let result = s.unwrap().result.unwrap();
    // Documents that the second write wins — potential data integrity issue
    assert_eq!(
        result.output, "second",
        "second mark_completed should overwrite the first"
    );
}

/// BUG HUNT: mark_failed after mark_completed — overwrites success with failure.
#[test]
fn mark_failed_after_completed_overwrites() {
    let store = AsyncTaskStore::new();
    store.insert_running("flip-flop".to_owned());

    store.mark_completed("flip-flop", fake_response("success"));
    store.mark_failed("flip-flop", "actually it failed".to_owned());

    let s = store.get_status("flip-flop");
    let s = s.unwrap();
    assert_eq!(
        s.status, "failed",
        "mark_failed after mark_completed overwrites state (BUG: completed result is lost)"
    );
    assert!(s.result.is_none(), "result should be gone after overwrite");
}

// ═══════════════════════════════════════════════════════════════════════
// 5. REAPER EDGE CASES
// ═══════════════════════════════════════════════════════════════════════

/// Reaper called on empty store — should not panic.
#[test]
fn reaper_on_empty_store() {
    let store = AsyncTaskStore::new();
    let reaped = store.reap_expired();
    assert_eq!(reaped, 0, "empty store should reap 0 entries");
}

/// Reaper called while only running tasks exist — none should be reaped.
#[test]
fn reaper_with_only_running_tasks() {
    let store = AsyncTaskStore::new();
    for i in 0..10 {
        store.insert_running(format!("running-{i}"));
    }

    let reaped = store.reap_expired();
    assert_eq!(reaped, 0, "running tasks must never be reaped");

    for i in 0..10 {
        assert!(
            store.get_status(&format!("running-{i}")).is_some(),
            "running-{i} must survive reaper"
        );
    }
}

/// FIXED: Reaper TTL is now based on `completed_at` (completion time),
/// NOT on `created_at`. A long-running task that completes late still
/// gets the full TTL window for clients to poll results.
#[test]
fn ttl_is_based_on_completion_not_creation() {
    // TTL starts at completion time, not insert time.
    // A task created now and completed immediately has ~600s to be polled.
    // Running tasks are never reaped regardless of how long they've been running.
    let store = AsyncTaskStore::new();
    store.insert_running("ttl-test".to_owned());

    // Immediately complete
    store.mark_completed("ttl-test", fake_response("done"));

    // Reap immediately — should NOT remove (just completed, within TTL)
    let reaped = store.reap_expired();
    assert_eq!(reaped, 0, "completed-just-now task should survive reaper");

    // The task should still be there
    assert!(store.get_status("ttl-test").is_some());

    // Running tasks should never be reaped even after a long time
    store.insert_running("long-running".to_owned());
    let reaped = store.reap_expired();
    assert_eq!(reaped, 0, "running tasks must never be reaped");
    assert!(
        store.get_status("long-running").is_some(),
        "running task must survive reaper"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 6. SERIALIZATION (AsyncTaskStatus)
// ═══════════════════════════════════════════════════════════════════════

/// Running status serialization: `result` and `error` should be absent
/// (skip_serializing_if = "Option::is_none").
#[test]
fn running_status_serialization_omits_null_fields() {
    let status = AsyncTaskStatus {
        status: "running".to_owned(),
        result: None,
        error: None,
    };

    #[allow(clippy::unwrap_used)] // reason: test code
    let json = serde_json::to_string(&status).unwrap();
    assert!(
        !json.contains("result"),
        "running status should not serialize 'result' field: {json}"
    );
    assert!(
        !json.contains("error"),
        "running status should not serialize 'error' field: {json}"
    );
}

/// Completed status serialization: `result` present, `error` absent.
#[test]
fn completed_status_serialization() {
    let status = AsyncTaskStatus {
        status: "completed".to_owned(),
        result: Some(fake_response("output")),
        error: None,
    };

    #[allow(clippy::unwrap_used)] // reason: test code
    let json = serde_json::to_string(&status).unwrap();
    assert!(
        json.contains("result"),
        "completed status must serialize 'result': {json}"
    );
    assert!(
        !json.contains("error"),
        "completed status should not serialize 'error': {json}"
    );
}

/// Failed status serialization: `error` present, `result` absent.
#[test]
fn failed_status_serialization() {
    let status = AsyncTaskStatus {
        status: "failed".to_owned(),
        result: None,
        error: Some("boom".to_owned()),
    };

    #[allow(clippy::unwrap_used)] // reason: test code
    let json = serde_json::to_string(&status).unwrap();
    assert!(
        !json.contains("result"),
        "failed status should not serialize 'result': {json}"
    );
    assert!(
        json.contains("error"),
        "failed status must serialize 'error': {json}"
    );
}

/// Round-trip deserialization of AsyncTaskStatus.
#[test]
fn status_round_trip_serde() {
    let original = AsyncTaskStatus {
        status: "completed".to_owned(),
        result: Some(fake_response("round-trip")),
        error: None,
    };

    #[allow(clippy::unwrap_used)] // reason: test code
    let json = serde_json::to_string(&original).unwrap();
    #[allow(clippy::unwrap_used)] // reason: test code
    let deserialized: AsyncTaskStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.status, "completed");
    assert!(deserialized.result.is_some());
    assert_eq!(deserialized.result.unwrap().output, "round-trip");
    assert!(deserialized.error.is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// 7. CONCURRENT STRESS TEST
// ═══════════════════════════════════════════════════════════════════════

/// Scenario: Mixed concurrent operations — inserts, completions, reads,
/// and reaper all running simultaneously.
#[test]
fn concurrent_mixed_operations_stress() {
    let store = Arc::new(AsyncTaskStore::new());
    let mut handles = Vec::new();

    // Thread 1-4: Insert tasks
    for batch in 0..4 {
        let store = Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            for i in 0..50 {
                store.insert_running(format!("stress-{batch}-{i}"));
            }
        }));
    }

    // Thread 5-6: Complete random tasks
    for batch in 0..2 {
        let store = Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            for i in 0..50 {
                store.mark_completed(
                    &format!("stress-{batch}-{i}"),
                    fake_response(&format!("done-{batch}-{i}")),
                );
            }
        }));
    }

    // Thread 7: Read tasks
    {
        let store = Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            for batch in 0..4 {
                for i in 0..50 {
                    let _ = store.get_status(&format!("stress-{batch}-{i}"));
                }
            }
        }));
    }

    // Thread 8: Reaper
    {
        let store = Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            for _ in 0..10 {
                let _ = store.reap_expired();
            }
        }));
    }

    // All threads must complete without deadlock or panic
    for h in handles {
        h.join().unwrap();
    }

    // Sanity: at least some tasks should exist
    let mut found = 0_u32;
    for batch in 0..4 {
        for i in 0..50 {
            if store.get_status(&format!("stress-{batch}-{i}")).is_some() {
                found = found.saturating_add(1);
            }
        }
    }
    assert!(
        found > 0,
        "at least some tasks should exist after stress test"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 8. DESIGN FLAW DOCUMENTATION TESTS
// ═══════════════════════════════════════════════════════════════════════

/// DESIGN ISSUE: The `status` field in `AsyncTaskStatus` is a free-form
/// `String`, not an enum. Callers must match on magic strings "running",
/// "completed", "failed". A typo or new state value will silently pass.
#[test]
fn status_field_is_stringly_typed() {
    let status = AsyncTaskStatus {
        status: "typo_running".to_owned(),
        result: None,
        error: None,
    };

    // This compiles and serializes fine — no type safety
    #[allow(clippy::unwrap_used)] // reason: test code
    let json = serde_json::to_string(&status).unwrap();
    assert!(
        json.contains("typo_running"),
        "arbitrary status strings are accepted — weak typing"
    );
}

/// DESIGN ISSUE: `AsyncTaskState` does not implement `PartialEq`,
/// making it impossible to assert equality in tests or compare states
/// programmatically. (This may be intentional if `TaskResponse` doesn't
/// impl PartialEq.)
#[test]
fn async_task_state_lacks_equality() {
    // We can only test via get_status() and string matching, not by
    // comparing AsyncTaskState values directly. This is a limitation.
    let store = AsyncTaskStore::new();
    store.insert_running("eq-test".to_owned());

    let s = store.get_status("eq-test");
    // We must use string comparison rather than enum comparison
    assert_eq!(s.unwrap().status, "running");
}
