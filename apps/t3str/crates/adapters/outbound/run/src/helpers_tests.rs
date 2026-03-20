use super::*;

#[test]
fn build_summary_empty() {
    let summary = build_summary(&[]);
    assert_eq!(summary.total, 0);
    assert_eq!(summary.passed, 0);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 0);
    assert_eq!(summary.errors, 0);
}

#[test]
fn build_summary_mixed() {
    let results = vec![
        TestResult {
            name: String::from("a"),
            status: TestStatus::Passed,
            duration_ms: None,
            message: None,
            file: None,
        },
        TestResult {
            name: String::from("b"),
            status: TestStatus::Failed,
            duration_ms: None,
            message: None,
            file: None,
        },
        TestResult {
            name: String::from("c"),
            status: TestStatus::Skipped,
            duration_ms: None,
            message: None,
            file: None,
        },
        TestResult {
            name: String::from("d"),
            status: TestStatus::Error,
            duration_ms: None,
            message: None,
            file: None,
        },
    ];
    let summary = build_summary(&results);
    assert_eq!(summary.total, 4);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.errors, 1);
}

#[test]
fn truncate_short_string() {
    let s = "hello";
    assert_eq!(truncate_output(s, 100), "hello");
}

#[test]
fn truncate_long_string() {
    let s = "abcdefghij";
    let result = truncate_output(s, 5);
    assert_eq!(result, "fghij");
}

#[test]
fn truncate_exact_length() {
    let s = "abcde";
    assert_eq!(truncate_output(s, 5), "abcde");
}
