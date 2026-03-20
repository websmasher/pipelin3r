use super::*;

#[test]
fn basic_output() {
    let output = "\
-- PASSED: tests/SecurityTxtTest.phpt
-- PASSED: tests/ParserTest.phpt
-- FAILED: tests/BrokenTest.phpt
-- SKIPPED: tests/SkipTest.phpt (reason)";

    let results = parse(output);
    assert_eq!(results.len(), 4);

    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("tests/SecurityTxtTest.phpt")
    );
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Passed));

    assert_eq!(results.get(1).map(|r| r.status), Some(TestStatus::Passed));

    assert_eq!(results.get(2).map(|r| r.status), Some(TestStatus::Failed));
    assert_eq!(
        results.get(2).map(|r| r.name.as_str()),
        Some("tests/BrokenTest.phpt")
    );

    assert_eq!(results.get(3).map(|r| r.status), Some(TestStatus::Skipped));
    assert_eq!(
        results.get(3).map(|r| r.name.as_str()),
        Some("tests/SkipTest.phpt")
    );
}

#[test]
fn skipped_with_reason_stripped() {
    let output = "-- SKIPPED: tests/Foo.phpt (needs PHP 8.2)";
    let results = parse(output);
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("tests/Foo.phpt")
    );
}

#[test]
fn empty_output() {
    let results = parse("");
    assert!(results.is_empty());
}

#[test]
fn non_matching_lines_ignored() {
    let output = "\
Starting tests...
OK (3 tests, 3 assertions)
-- PASSED: tests/One.phpt";

    let results = parse(output);
    assert_eq!(results.len(), 1);
}
