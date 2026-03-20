use super::*;

/// Fallback value for test assertions when `.get()` returns `None`.
fn fallback() -> TestResult {
    TestResult {
        name: String::new(),
        status: TestStatus::Skipped,
        duration_ms: None,
        message: None,
        file: None,
    }
}

#[test]
fn parse_basic_junit_xml() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites>
  <testsuite name="tests" tests="4" errors="1" failures="1">
    <testcase classname="tests.test_module" name="test_passing" time="0.001">
    </testcase>
    <testcase classname="tests.test_module" name="test_failing" time="0.002">
      <failure message="AssertionError: expected true">traceback here</failure>
    </testcase>
    <testcase classname="tests.test_module" name="test_skipped" time="0.000">
      <skipped message="not applicable"/>
    </testcase>
    <testcase classname="tests.test_module" name="test_error" time="0.003">
      <error message="RuntimeError: boom">traceback here</error>
    </testcase>
  </testsuite>
</testsuites>"#;

    let results = parse(xml);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.unwrap_or_default();
    assert_eq!(results.len(), 4, "should have 4 test results");

    let fb = fallback();

    let first = results.first().unwrap_or(&fb);
    assert_eq!(first.name, "tests.test_module::test_passing");
    assert_eq!(first.status, TestStatus::Passed);
    assert_eq!(first.duration_ms, Some(1));
    assert!(
        first.message.is_none(),
        "passed test should have no message"
    );

    let second = results.get(1).unwrap_or(&fb);
    assert_eq!(second.name, "tests.test_module::test_failing");
    assert_eq!(second.status, TestStatus::Failed);
    assert_eq!(second.duration_ms, Some(2));
    assert_eq!(
        second.message.as_deref(),
        Some("AssertionError: expected true")
    );

    let third = results.get(2).unwrap_or(&fb);
    assert_eq!(third.name, "tests.test_module::test_skipped");
    assert_eq!(third.status, TestStatus::Skipped);
    assert_eq!(third.message.as_deref(), Some("not applicable"));

    let fourth = results.get(3).unwrap_or(&fb);
    assert_eq!(fourth.name, "tests.test_module::test_error");
    assert_eq!(fourth.status, TestStatus::Error);
    assert_eq!(fourth.message.as_deref(), Some("RuntimeError: boom"));
}

#[test]
fn parse_without_testsuites_wrapper() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuite name="tests" tests="1">
    <testcase classname="tests.test_module" name="test_one" time="0.005">
    </testcase>
</testsuite>"#;

    let results = parse(xml);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.unwrap_or_default();
    assert_eq!(results.len(), 1, "should have 1 test result");
    if let Some(r) = results.first() {
        assert_eq!(r.name, "tests.test_module::test_one");
        assert_eq!(r.status, TestStatus::Passed);
        assert_eq!(r.duration_ms, Some(5));
    }
}

#[test]
fn parse_empty_classname() {
    let xml = r#"<testsuite name="tests" tests="1">
    <testcase classname="" name="standalone_test" time="0.010">
    </testcase>
</testsuite>"#;

    let results = parse(xml);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.unwrap_or_default();
    assert_eq!(results.len(), 1, "should have 1 test result");
    if let Some(r) = results.first() {
        assert_eq!(r.name, "standalone_test");
    }
}

#[test]
fn parse_missing_time_attribute() {
    let xml = r#"<testsuite name="tests" tests="1">
    <testcase classname="mod" name="test_no_time">
    </testcase>
</testsuite>"#;

    let results = parse(xml);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.unwrap_or_default();
    assert_eq!(results.len(), 1, "should have 1 test result");
    if let Some(r) = results.first() {
        assert_eq!(r.name, "mod::test_no_time");
        assert!(
            r.duration_ms.is_none(),
            "duration should be None when time attr is missing"
        );
    }
}

#[test]
fn parse_empty_xml() {
    let results = parse("");
    assert!(results.is_ok(), "empty XML should return Ok");
    let results = results.unwrap_or_default();
    assert!(results.is_empty(), "empty XML should return empty vec");
}

#[test]
fn parse_whitespace_only_xml() {
    let results = parse("   \n\t  ");
    assert!(results.is_ok(), "whitespace-only XML should return Ok");
    let results = results.unwrap_or_default();
    assert!(
        results.is_empty(),
        "whitespace-only XML should return empty vec"
    );
}

#[test]
fn parse_malformed_xml() {
    let xml = "<testsuite><testcase name='foo'><not closed";
    let results = parse(xml);
    // Malformed XML may or may not produce an error depending on what the
    // parser encounters. The key requirement is it does not panic.
    let _ = results;
}

#[test]
fn parse_self_closing_testcase() {
    let xml = r#"<testsuite name="tests" tests="1">
    <testcase classname="mod" name="test_ok" time="0.001"/>
</testsuite>"#;

    let results = parse(xml);
    assert!(results.is_ok(), "parse should succeed");
    let results = results.unwrap_or_default();
    assert_eq!(results.len(), 1, "should have 1 test result");
    if let Some(r) = results.first() {
        assert_eq!(r.name, "mod::test_ok");
        assert_eq!(r.status, TestStatus::Passed);
    }
}
