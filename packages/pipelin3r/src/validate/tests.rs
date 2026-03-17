#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::type_complexity,
    reason = "test code: explicit types for clarity"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct filesystem cleanup in tests"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

/// Helper: a validator that passes on the Nth call (1-indexed).
fn pass_on_nth(
    pass_on: u32,
) -> impl Fn(&Path) -> Pin<Box<dyn Future<Output = Result<ValidationReport, PipelineError>> + Send + '_>>
+ Send
+ Sync {
    let counter = Arc::new(AtomicU32::new(0));
    move |_path: &Path| {
        let counter = Arc::clone(&counter);
        Box::pin(async move {
            let n = counter.fetch_add(1, Ordering::SeqCst).saturating_add(1);
            if n >= pass_on {
                Ok(ValidationReport::pass())
            } else {
                Ok(ValidationReport::fail(vec![ValidationFinding::new(
                    "test",
                    format!("failing on iteration {n}"),
                )]))
            }
        })
    }
}

/// Helper: a validator that always fails.
fn always_fail()
-> impl Fn(&Path) -> Pin<Box<dyn Future<Output = Result<ValidationReport, PipelineError>> + Send + '_>>
+ Send
+ Sync {
    |_path: &Path| {
        Box::pin(async {
            Ok(ValidationReport::fail(vec![ValidationFinding::new(
                "always",
                "always fails",
            )]))
        })
    }
}

/// Helper: a validator that always passes.
fn always_pass()
-> impl Fn(&Path) -> Pin<Box<dyn Future<Output = Result<ValidationReport, PipelineError>> + Send + '_>>
+ Send
+ Sync {
    |_path: &Path| Box::pin(async { Ok(ValidationReport::pass()) })
}

/// Helper: a strategy that returns a `FunctionFix` (no-op) for each finding.
fn noop_fix_strategy() -> impl Fn(&ValidationReport, u32) -> Vec<RemediationAction> + Send + Sync {
    |report: &ValidationReport, _iteration: u32| {
        if report.findings.is_empty() && report.raw_output.is_some() {
            // Raw output failure — still provide a fix action.
            return vec![RemediationAction::FunctionFix(Box::new(|| {
                Box::pin(async { Ok(()) })
            }))];
        }
        report
            .findings
            .iter()
            .map(|_| RemediationAction::FunctionFix(Box::new(|| Box::pin(async { Ok(()) }))))
            .collect()
    }
}

/// Helper: a strategy that returns no actions (gives up).
fn give_up_strategy() -> impl Fn(&ValidationReport, u32) -> Vec<RemediationAction> + Send + Sync {
    |_report: &ValidationReport, _iteration: u32| Vec::new()
}

/// Helper: a strategy that skips everything.
fn skip_strategy() -> impl Fn(&ValidationReport, u32) -> Vec<RemediationAction> + Send + Sync {
    |report: &ValidationReport, _iteration: u32| {
        report
            .findings
            .iter()
            .map(|_| RemediationAction::Skip {
                reason: String::from("test skip"),
            })
            .collect()
    }
}

// ── Report tests ──

#[test]
fn report_pass_has_no_findings() {
    let report = ValidationReport::pass();
    assert!(report.passed, "pass() should set passed=true");
    assert!(report.findings.is_empty(), "pass() should have no findings");
    assert!(
        report.raw_output.is_none(),
        "pass() should have no raw output"
    );
}

#[test]
fn report_fail_raw_captures_output() {
    let report = ValidationReport::fail_raw("error: type mismatch");
    assert!(!report.passed, "fail_raw should set passed=false");
    assert!(
        report.findings.is_empty(),
        "fail_raw should have no structured findings"
    );
    assert_eq!(
        report.raw_output.as_deref(),
        Some("error: type mismatch"),
        "fail_raw should capture raw output"
    );
}

#[test]
fn report_fail_captures_findings() {
    let findings = vec![
        ValidationFinding::new("lint", "unused variable"),
        ValidationFinding::with_key("type", "wrong return type", "src/main.rs"),
    ];
    let report = ValidationReport::fail(findings);
    assert!(!report.passed, "fail should set passed=false");
    assert_eq!(report.findings.len(), 2, "should have 2 findings");
    assert!(
        report.raw_output.is_none(),
        "fail should have no raw output"
    );
}

#[test]
fn findings_with_tag_filters_correctly() {
    let findings = vec![
        ValidationFinding::new("lint", "unused var"),
        ValidationFinding::new("type", "mismatch"),
        ValidationFinding::new("lint", "dead code"),
    ];
    let report = ValidationReport::fail(findings);
    let lint_findings = report.findings_with_tag("lint");
    assert_eq!(lint_findings.len(), 2, "should find 2 lint findings");
    let type_findings = report.findings_with_tag("type");
    assert_eq!(type_findings.len(), 1, "should find 1 type finding");
    let none_findings = report.findings_with_tag("missing");
    assert!(none_findings.is_empty(), "should find 0 missing findings");
}

#[test]
fn to_markdown_pass() {
    let report = ValidationReport::pass();
    let md = report.to_markdown();
    assert!(md.contains("PASSED"), "pass markdown should say PASSED");
}

#[test]
fn to_markdown_fail_with_findings() {
    let findings = vec![ValidationFinding::with_key("lint", "unused var", "lib.rs")];
    let report = ValidationReport::fail(findings);
    let md = report.to_markdown();
    assert!(md.contains("FAILED"), "should say FAILED");
    assert!(md.contains("[lint]"), "should contain tag");
    assert!(md.contains("`lib.rs`"), "should contain item key");
    assert!(md.contains("unused var"), "should contain message");
}

#[test]
fn to_markdown_fail_with_raw_output() {
    let report = ValidationReport::fail_raw("compile error on line 42");
    let md = report.to_markdown();
    assert!(md.contains("FAILED"), "should say FAILED");
    assert!(
        md.contains("compile error on line 42"),
        "should contain raw output"
    );
    assert!(md.contains("```"), "should have code block");
}

// ── Finding construction tests ──

#[test]
fn finding_new_has_no_key() {
    let f = ValidationFinding::new("tag", "msg");
    assert_eq!(f.tag, "tag");
    assert_eq!(f.message, "msg");
    assert!(f.item_key.is_none());
}

#[test]
fn finding_with_key_has_key() {
    let f = ValidationFinding::with_key("tag", "msg", "key");
    assert_eq!(f.tag, "tag");
    assert_eq!(f.message, "msg");
    assert_eq!(f.item_key.as_deref(), Some("key"));
}

// ── ValidateConfig tests ──

#[test]
fn validate_config_new_defaults() {
    let cfg = ValidateConfig::new("test", PathBuf::from("/tmp/work"));
    assert_eq!(cfg.name, "test");
    assert_eq!(cfg.work_dir, PathBuf::from("/tmp/work"));
    assert_eq!(cfg.max_iterations, 3);
    assert_eq!(
        cfg.fix_agent_defaults.work_dir,
        Some(PathBuf::from("/tmp/work"))
    );
}

// ── validate_and_fix loop tests ──
// These tests use mock validators and strategies (no real executor calls).
// The executor is constructed in dry-run mode so run_agent captures to disk
// instead of making HTTP calls.

#[tokio::test]
async fn converges_on_first_pass() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-validate-test-pass"));

    let config = ValidateConfig::new("first-pass", PathBuf::from("/tmp/work"));

    let result = validate_and_fix(&executor, &config, always_pass(), noop_fix_strategy())
        .await
        .unwrap();

    assert!(result.converged, "should converge on first pass");
    assert_eq!(result.iterations, 1, "should take 1 iteration");
    assert!(
        result.final_report.passed,
        "final report should show passed"
    );
    assert_eq!(result.history.len(), 1, "should have 1 report in history");

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-validate-test-pass");
}

#[tokio::test]
async fn exhausts_iterations() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-validate-test-exhaust"));

    let mut config = ValidateConfig::new("exhaust", PathBuf::from("/tmp/work"));
    config.max_iterations = 2;

    let result = validate_and_fix(&executor, &config, always_fail(), skip_strategy())
        .await
        .unwrap();

    assert!(!result.converged, "should NOT converge");
    assert_eq!(result.iterations, 2, "should run exactly max_iterations");
    assert!(
        !result.final_report.passed,
        "final report should show failed"
    );
    assert_eq!(
        result.history.len(),
        2,
        "should have max_iterations reports"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-validate-test-exhaust");
}

#[tokio::test]
async fn strategy_gives_up_early() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-validate-test-giveup"));

    let mut config = ValidateConfig::new("give-up", PathBuf::from("/tmp/work"));
    config.max_iterations = 10;

    let result = validate_and_fix(&executor, &config, always_fail(), give_up_strategy())
        .await
        .unwrap();

    assert!(
        !result.converged,
        "should NOT converge when strategy gives up"
    );
    assert_eq!(
        result.iterations, 1,
        "should stop after first iteration (no actions)"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-validate-test-giveup");
}

#[tokio::test]
async fn converges_on_second_iteration() {
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(PathBuf::from("/tmp/pipelin3r-validate-test-second"));

    let mut config = ValidateConfig::new("second-pass", PathBuf::from("/tmp/work"));
    config.max_iterations = 5;

    let result = validate_and_fix(&executor, &config, pass_on_nth(2), noop_fix_strategy())
        .await
        .unwrap();

    assert!(result.converged, "should converge on second iteration");
    assert_eq!(result.iterations, 2, "should take 2 iterations");
    assert!(
        result.final_report.passed,
        "final report should show passed"
    );
    assert_eq!(result.history.len(), 2, "should have 2 reports in history");
    assert!(
        !result.history.first().unwrap().passed,
        "first report should show failed"
    );

    let _ = std::fs::remove_dir_all("/tmp/pipelin3r-validate-test-second");
}

#[tokio::test]
async fn require_converged_ok() {
    let result = ValidateResult {
        converged: true,
        iterations: 1,
        final_report: ValidationReport::pass(),
        history: vec![ValidationReport::pass()],
    };
    assert!(
        result.require_converged().is_ok(),
        "require_converged should return Ok when converged"
    );
}

#[tokio::test]
async fn require_converged_err() {
    let result = ValidateResult {
        converged: false,
        iterations: 3,
        final_report: ValidationReport::fail(vec![ValidationFinding::new("test", "still broken")]),
        history: Vec::new(),
    };
    let err = result.require_converged();
    assert!(
        err.is_err(),
        "require_converged should return Err when not converged"
    );
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("still broken"),
        "error should describe remaining errors: {msg}"
    );
}

#[tokio::test]
async fn require_converged_err_raw_output() {
    let result = ValidateResult {
        converged: false,
        iterations: 2,
        final_report: ValidationReport::fail_raw("compile error"),
        history: Vec::new(),
    };
    let err = result.require_converged();
    assert!(err.is_err(), "should return Err");
    let msg = err.unwrap_err().to_string();
    assert!(
        msg.contains("compile error"),
        "error should contain raw output: {msg}"
    );
}

// ── RemediationAction debug tests ──

#[test]
fn remediation_action_debug_agent_fix() {
    let action = RemediationAction::AgentFix {
        prompt: String::from("fix it"),
        work_dir_override: None,
    };
    let debug = format!("{action:?}");
    assert!(
        debug.contains("AgentFix"),
        "debug should contain variant name"
    );
    assert!(debug.contains("fix it"), "debug should contain prompt");
}

#[test]
fn remediation_action_debug_function_fix() {
    let action = RemediationAction::FunctionFix(Box::new(|| Box::pin(async { Ok(()) })));
    let debug = format!("{action:?}");
    assert!(
        debug.contains("FunctionFix"),
        "debug should contain variant name"
    );
}

#[test]
fn remediation_action_debug_skip() {
    let action = RemediationAction::Skip {
        reason: String::from("not relevant"),
    };
    let debug = format!("{action:?}");
    assert!(debug.contains("Skip"), "debug should contain variant name");
    assert!(
        debug.contains("not relevant"),
        "debug should contain reason"
    );
}
