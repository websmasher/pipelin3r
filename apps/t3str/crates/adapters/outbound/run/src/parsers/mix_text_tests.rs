use super::*;

#[test]
fn parse_failures() {
    let output = "\
..F.S

  1) test something (MyTest)
     test/my_test.exs:5
     Assertion with == failed
     code:  assert 1 == 2
     left:  1
     right: 2

  2) test another thing (OtherTest)
     test/other_test.exs:10

Finished in 0.1 seconds (0.1s on load, 0.0s on tests)
4 tests, 2 failures, 1 skipped";

    let results = parse(output);
    assert_eq!(results.len(), 2);

    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("MyTest.something")
    );
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Failed));

    assert_eq!(
        results.get(1).map(|r| r.name.as_str()),
        Some("OtherTest.another thing")
    );
    assert_eq!(results.get(1).map(|r| r.status), Some(TestStatus::Failed));
}

#[test]
fn parse_summary_line() {
    let output = "\
Finished in 0.1 seconds (0.1s on load, 0.0s on tests)
4 tests, 1 failure, 1 skipped";

    let summary = parse_summary(output);
    assert_eq!(summary, Some((4, 1, 1)));
}

#[test]
fn parse_summary_no_skipped() {
    let output = "10 tests, 0 failures";
    let summary = parse_summary(output);
    assert_eq!(summary, Some((10, 0, 0)));
}

#[test]
fn clean_run_no_failures() {
    let output = "\
....

Finished in 0.05 seconds (0.04s on load, 0.01s on tests)
4 tests, 0 failures";

    let results = parse(output);
    assert!(results.is_empty());

    let summary = parse_summary(output);
    assert_eq!(summary, Some((4, 0, 0)));
}

#[test]
fn empty_output() {
    let results = parse("");
    assert!(results.is_empty());
    assert_eq!(parse_summary(""), None);
}
