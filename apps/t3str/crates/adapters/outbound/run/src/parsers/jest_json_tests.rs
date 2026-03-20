use super::*;

#[test]
fn standard_jest_output() {
    let json = r#"{
        "testResults": [
            {
                "testFilePath": "/path/to/test.js",
                "testResults": [
                    {"title": "passes", "status": "passed", "duration": 42},
                    {"title": "fails", "status": "failed", "duration": 10, "failureMessages": ["expected true"]},
                    {"title": "pending", "status": "pending", "duration": 0}
                ]
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
        Some("passes"),
        "name from title"
    );
    assert_eq!(
        results.first().and_then(|r| r.duration_ms),
        Some(42),
        "duration"
    );
    assert_eq!(
        results.first().and_then(|r| r.file.as_deref()),
        Some("/path/to/test.js"),
        "file path"
    );

    assert_eq!(
        results.get(1).map(|r| r.status),
        Some(TestStatus::Failed),
        "second is failed"
    );
    assert_eq!(
        results.get(1).and_then(|r| r.message.as_deref()),
        Some("expected true"),
        "failure message"
    );

    assert_eq!(
        results.get(2).map(|r| r.status),
        Some(TestStatus::Skipped),
        "pending maps to skipped"
    );
}

#[test]
fn alternate_assertion_results_format() {
    let json = r#"{
        "testResults": [
            {
                "testFilePath": "/path/to/test.js",
                "assertionResults": [
                    {"title": "works", "status": "passed", "duration": 5}
                ]
            }
        ]
    }"#;

    let results = parse(json);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.ok().unwrap_or_default();
    assert_eq!(results.len(), 1, "should parse assertionResults");
}

#[test]
fn empty_test_results() {
    let json = r#"{"testResults": []}"#;
    let results = parse(json);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.ok().unwrap_or_default();
    assert!(results.is_empty(), "empty testResults => empty results");
}

#[test]
fn invalid_json_returns_parse_failed() {
    let result = parse("not json");
    assert!(result.is_err(), "should fail on invalid JSON");
}
