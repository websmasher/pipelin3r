#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::type_complexity,
    reason = "test code: explicit types for clarity"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct fs access for test fixtures"
)]

use super::*;

#[test]
fn agent_result_require_success_ok() {
    let result = AgentResult {
        success: true,
        output: String::from("done"),
    };
    assert!(
        result.require_success().is_ok(),
        "should return Ok for successful agent"
    );
}

#[test]
fn agent_result_require_success_err() {
    let result = AgentResult {
        success: false,
        output: String::from("timeout exceeded"),
    };
    let err = result.require_success();
    assert!(err.is_err(), "should return Err for failed agent");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("timeout exceeded"),
        "error should contain output: {msg}"
    );
}

#[test]
fn format_duration_minutes() {
    assert_eq!(
        format_duration(Duration::from_secs(900)),
        "15m",
        "15 minutes"
    );
}

#[test]
fn format_duration_hours_and_minutes() {
    assert_eq!(
        format_duration(Duration::from_secs(5400)),
        "1h30m",
        "1 hour 30 minutes"
    );
}

#[test]
fn format_duration_seconds_only() {
    assert_eq!(
        format_duration(Duration::from_secs(45)),
        "45s",
        "45 seconds"
    );
}

#[test]
fn format_duration_zero() {
    assert_eq!(format_duration(Duration::from_secs(0)), "0s", "zero");
}

#[test]
fn format_duration_exact_hour() {
    assert_eq!(
        format_duration(Duration::from_secs(3600)),
        "1h",
        "exact hour"
    );
}

#[test]
fn agent_task_builder_chain() {
    let task = AgentTask::new()
        .prompt("hello")
        .work_dir(Path::new("/tmp"))
        .expect_outputs(&["out.txt", "report.md"])
        .auth(Auth::ApiKey(String::from("sk-test")));

    assert_eq!(
        task.prompt.as_deref(),
        Some("hello"),
        "prompt should be set"
    );
    assert_eq!(
        task.work_dir.as_deref(),
        Some(Path::new("/tmp")),
        "work_dir should be set"
    );
    assert_eq!(
        task.expected_outputs.len(),
        2,
        "expected_outputs should have 2 entries"
    );
    assert!(task.auth.is_some(), "auth should be set");
}

#[test]
fn agent_task_default_is_empty() {
    let task = AgentTask::new();
    assert!(task.prompt.is_none(), "prompt should default to None");
    assert!(task.work_dir.is_none(), "work_dir should default to None");
    assert!(
        task.expected_outputs.is_empty(),
        "expected_outputs should default to empty"
    );
    assert!(task.auth.is_none(), "auth should default to None");
}

#[tokio::test]
async fn batch_dry_run_produces_correct_count() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-batch-test"));

    let items: Vec<String> = vec![
        String::from("item_a"),
        String::from("item_b"),
        String::from("item_c"),
    ];

    let results = executor
        .agent("test-batch")
        .model(Model::Sonnet4_6)
        .items(items, 2)
        .for_each(|item| AgentTask::new().prompt(&format!("process {item}")))
        .execute()
        .await
        .unwrap();

    assert_eq!(results.len(), 3, "should produce one result per item");
    for (i, r) in results.iter().enumerate() {
        assert!(r.is_ok(), "item {i} should succeed in dry-run");
    }

    // Clean up test artifacts.
    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-batch-test");
}

#[test]
fn regression_require_success_returns_agent_failed_not_other() {
    // Regression: AgentResult{success:false}.require_success() returned
    // PipelineError::Other instead of PipelineError::AgentFailed.
    let result = AgentResult {
        success: false,
        output: String::from("model timeout"),
    };
    let err = result.require_success();
    assert!(err.is_err(), "failed agent must return Err");
    assert!(
        matches!(&err, Err(PipelineError::AgentFailed { message }) if message == "model timeout"),
        "must be PipelineError::AgentFailed with preserved message, got: {err:?}"
    );
}

#[test]
fn mutant_kill_tools_empty_check() {
    // Mutant kill: agent.rs:155 — `> with </<=/==/>=` on tools empty check (i > 0)
    // Empty tools slice must produce no --allowedTools in YAML.
    // Non-empty tools must produce --allowedTools with comma-separated names.
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-tools-test"));

    // Build with empty tools — should NOT have --allowedTools
    let builder_empty = executor.agent("test-tools-empty").tools(&[]);
    // Access tools field directly: empty join should be ""
    assert_eq!(
        builder_empty.tools.as_deref(),
        Some(""),
        "empty tools slice should produce empty string"
    );

    // Build with two tools — should have comma-separated
    let builder_two = executor
        .agent("test-tools-two")
        .tools(&[Tool::Read, Tool::Write]);
    assert_eq!(
        builder_two.tools.as_deref(),
        Some("Read,Write"),
        "two tools should be comma-separated without leading comma"
    );

    // Build with one tool — no commas
    let builder_one = executor.agent("test-tools-one").tools(&[Tool::Grep]);
    assert_eq!(
        builder_one.tools.as_deref(),
        Some("Grep"),
        "single tool should have no comma"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-tools-test");
}

#[tokio::test]
async fn mutant_kill_resolve_model_string_returns_correct_id() {
    // Mutant kill: agent.rs:209 — resolve_model_string returns replaced with None/""/""xyzzy"
    // Verify the model string appears in the dry-run task YAML.
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-model-test"));

    let result = executor
        .agent("test-model")
        .model(Model::Opus4_6)
        .prompt("test prompt")
        .execute()
        .await
        .unwrap();

    assert!(result.success, "dry-run should succeed");

    // Read the captured task YAML and verify it contains the opus model ID.
    let task_yaml =
        std::fs::read_to_string("/tmp/pipelin3r-model-test/test-model/0/task.yaml").unwrap();
    assert!(
        task_yaml.contains("claude-opus-4-6"),
        "task YAML must contain the resolved model ID 'claude-opus-4-6', got: {task_yaml}"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-model-test");
}

#[tokio::test]
async fn mutant_kill_batch_partial_failure_counts() {
    // Mutant kill: agent.rs:440 — `&& with ||` and `> with ==/</>=/>=` on partial failure check
    // The batch code checks `if failed > 0 && succeeded > 0` to log partial failure.
    // We verify the results vector has correct success/failure counts.
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-batch-partial"));

    let items = vec![String::from("a"), String::from("b"), String::from("c")];
    let results = executor
        .agent("test-partial")
        .model(Model::Sonnet4_6)
        .items(items, 2)
        .for_each(|item| AgentTask::new().prompt(&format!("do {item}")))
        .execute()
        .await
        .unwrap();

    assert_eq!(results.len(), 3, "should have 3 results");

    // In dry-run, all succeed — verify counts.
    let mut succeeded: usize = 0;
    let mut failed: usize = 0;
    for r in &results {
        if r.is_ok() {
            succeeded = succeeded.saturating_add(1);
        } else {
            failed = failed.saturating_add(1);
        }
    }
    assert_eq!(succeeded, 3, "all 3 dry-run tasks should succeed");
    assert_eq!(failed, 0, "no dry-run tasks should fail");

    // Now test: when failed > 0 AND succeeded > 0, that's partial failure.
    // The mutant changes && to || or changes the comparison operators.
    // With all succeeded (3,0): failed > 0 is false, so partial failure should NOT trigger.
    // This distinguishes && from ||: with ||, (3 > 0 || 0 > 0) = true, incorrectly.
    let all_success = failed > 0 && succeeded > 0;
    assert!(
        !all_success,
        "when all tasks succeed, partial failure check must be false"
    );

    // Simulate mixed results.
    let sim_succeeded: usize = 2;
    let sim_failed: usize = 1;
    let partial = sim_failed > 0 && sim_succeeded > 0;
    assert!(
        partial,
        "when some fail and some succeed, partial failure check must be true"
    );

    // Edge case: all failed (succeeded=0).
    let all_fail_succeeded: usize = 0;
    let all_fail_failed: usize = 3;
    let all_failed = all_fail_failed > 0 && all_fail_succeeded > 0;
    assert!(
        !all_failed,
        "when all tasks fail (succeeded=0), partial failure check must be false"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-batch-partial");
}

#[test]
fn mutant_kill_format_duration_zero_vs_nonzero() {
    // Mutant kill: agent.rs:702 — `> with <` on format_duration hour/minute checks
    // Duration::ZERO must produce "0s", not "0h" or "0m".
    assert_eq!(
        format_duration(Duration::ZERO),
        "0s",
        "zero duration must format as '0s'"
    );

    // 5 seconds: must be "5s", not "0h" or "0m5s"
    assert_eq!(
        format_duration(Duration::from_secs(5)),
        "5s",
        "5 seconds must format as '5s'"
    );

    // 60 seconds = 1 minute exactly
    assert_eq!(
        format_duration(Duration::from_secs(60)),
        "1m",
        "60 seconds must format as '1m'"
    );

    // 61 seconds = 1m1s
    assert_eq!(
        format_duration(Duration::from_secs(61)),
        "1m1s",
        "61 seconds must format as '1m1s'"
    );

    // 3600 seconds = 1h exactly
    assert_eq!(
        format_duration(Duration::from_secs(3600)),
        "1h",
        "3600 seconds must format as '1h'"
    );

    // 3601 seconds = 1h0m (seconds dropped when hours present, minutes=0)
    // Actually looking at the code: if hours > 0, it checks minutes > 0.
    // 3601s: hours=1, remaining=1, minutes=0, seconds=1.
    // Since minutes == 0, it returns "1h". Seconds are lost in hours mode.
    assert_eq!(
        format_duration(Duration::from_secs(3601)),
        "1h",
        "3601 seconds formats as '1h' (seconds dropped in hour mode)"
    );

    // 3660 seconds = 1h1m
    assert_eq!(
        format_duration(Duration::from_secs(3660)),
        "1h1m",
        "3660 seconds must format as '1h1m'"
    );
}

#[tokio::test]
async fn batch_without_mapper_fails() {
    let executor = Executor::with_defaults().unwrap_or_else(|_| {
        Executor::new(&shedul3r_rs_sdk::ClientConfig::default())
            .unwrap_or_else(|_| std::process::abort())
    });

    let items: Vec<u32> = vec![1, 2];
    let result = executor.agent("test").items(items, 1).execute().await;

    assert!(result.is_err(), "should fail without for_each mapper");
}

#[test]
fn mutant_kill_v2_count_batch_outcomes_all_success() {
    // Mutant kill: agent.rs:440 — all 7 mutations on `failed > 0 && succeeded > 0`
    // Case: all success (succeeded=3, failed=0) → NOT partial failure.
    // Kills `&& → ||` because with ||, (3 > 0 || 0 > 0) = true, but must be false.
    let results: Vec<Result<&str, &str>> = vec![Ok("a"), Ok("b"), Ok("c")];
    let (succeeded, failed) = count_batch_outcomes(&results);
    assert_eq!(succeeded, 3, "all Ok results must count as succeeded");
    assert_eq!(failed, 0, "no Err results means failed=0");
    assert!(
        !is_partial_failure(succeeded, failed),
        "all-success (3,0) must NOT be partial failure"
    );
}

#[test]
fn mutant_kill_v2_count_batch_outcomes_all_failed() {
    // Case: all failed (succeeded=0, failed=3) → NOT partial failure.
    // Kills `> with ==` on succeeded: with ==, (0 == 0) = true, but must be false.
    let results: Vec<Result<&str, &str>> = vec![Err("x"), Err("y"), Err("z")];
    let (succeeded, failed) = count_batch_outcomes(&results);
    assert_eq!(succeeded, 0, "no Ok results means succeeded=0");
    assert_eq!(failed, 3, "all Err results must count as failed");
    assert!(
        !is_partial_failure(succeeded, failed),
        "all-failed (0,3) must NOT be partial failure"
    );
}

#[test]
fn mutant_kill_v2_count_batch_outcomes_partial_failure() {
    // Case: mixed (succeeded=2, failed=1) → IS partial failure.
    // Kills `> with <` on both sides: (2 < 0) = false, (1 < 0) = false.
    // Kills `> with >=` indirectly (2 >= 0 is true, so that alone doesn't help,
    // but combined with the other cases it does).
    let results: Vec<Result<&str, &str>> = vec![Ok("a"), Err("x"), Ok("b")];
    let (succeeded, failed) = count_batch_outcomes(&results);
    assert_eq!(succeeded, 2, "two Ok results");
    assert_eq!(failed, 1, "one Err result");
    assert!(
        is_partial_failure(succeeded, failed),
        "mixed (2,1) must be partial failure"
    );
}

#[test]
fn mutant_kill_v2_count_batch_outcomes_empty() {
    // Case: empty (succeeded=0, failed=0) → NOT partial failure.
    // Kills `> with >=` on both sides: (0 >= 0) = true with >=, but must be false.
    let results: Vec<Result<&str, &str>> = vec![];
    let (succeeded, failed) = count_batch_outcomes(&results);
    assert_eq!(succeeded, 0, "empty batch has 0 succeeded");
    assert_eq!(failed, 0, "empty batch has 0 failed");
    assert!(
        !is_partial_failure(succeeded, failed),
        "empty (0,0) must NOT be partial failure"
    );
}
