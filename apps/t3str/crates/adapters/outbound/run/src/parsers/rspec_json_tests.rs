use super::*;

#[test]
fn basic_rspec_output() {
    let json = r#"{
        "examples": [
            {
                "full_description": "SomeClass does something",
                "status": "passed",
                "run_time": 0.001,
                "file_path": "./spec/some_spec.rb",
                "line_number": 5
            },
            {
                "full_description": "SomeClass fails",
                "status": "failed",
                "run_time": 0.002,
                "exception": {"message": "expected true, got false"}
            },
            {
                "full_description": "SomeClass is pending",
                "status": "pending",
                "run_time": 0.0
            }
        ]
    }"#;

    let results = parse(json);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.ok().unwrap_or_default();
    assert_eq!(results.len(), 3, "should have 3 results");

    assert_eq!(
        results.first().map(|r| r.status),
        Some(TestStatus::Passed),
        "first is passed"
    );
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("SomeClass does something"),
        "name from full_description"
    );
    assert_eq!(
        results.first().and_then(|r| r.file.as_deref()),
        Some("./spec/some_spec.rb"),
        "file path"
    );
    assert_eq!(
        results.first().and_then(|r| r.duration_ms),
        Some(1),
        "duration in ms"
    );

    assert_eq!(
        results.get(1).map(|r| r.status),
        Some(TestStatus::Failed),
        "second is failed"
    );
    assert_eq!(
        results.get(1).and_then(|r| r.message.as_deref()),
        Some("expected true, got false"),
        "exception message"
    );

    assert_eq!(
        results.get(2).map(|r| r.status),
        Some(TestStatus::Skipped),
        "pending maps to skipped"
    );
}

#[test]
fn missing_exception_field() {
    let json = r#"{
        "examples": [
            {
                "full_description": "works fine",
                "status": "passed",
                "run_time": 0.01
            }
        ]
    }"#;

    let results = parse(json);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.ok().unwrap_or_default();
    assert_eq!(results.len(), 1, "should have 1 result");
    assert!(
        results.first().and_then(|r| r.message.as_ref()).is_none(),
        "no message when no exception"
    );
}

#[test]
fn empty_examples() {
    let json = r#"{"examples": []}"#;
    let results = parse(json);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.ok().unwrap_or_default();
    assert!(results.is_empty(), "empty examples => empty results");
}
