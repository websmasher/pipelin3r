use super::*;

#[test]
fn basic_output() {
    let output = "\
  Determining projects to restore...
  All projects are up-to-date for restore.
  Passed TestNamespace.TestClass.TestMethod1 [5 ms]
  Passed TestNamespace.TestClass.TestMethod2 [< 1 ms]
  Failed TestNamespace.TestClass.TestMethod3 [10 ms]
    Expected: True
    Actual:   False
  Skipped TestNamespace.TestClass.TestMethod4";

    let results = parse(output);
    assert_eq!(results.len(), 4);

    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("TestNamespace.TestClass.TestMethod1")
    );
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Passed));
    assert_eq!(results.first().and_then(|r| r.duration_ms), Some(5));

    assert_eq!(results.get(1).and_then(|r| r.duration_ms), Some(0));

    assert_eq!(results.get(2).map(|r| r.status), Some(TestStatus::Failed));
    assert_eq!(results.get(2).and_then(|r| r.duration_ms), Some(10));

    assert_eq!(results.get(3).map(|r| r.status), Some(TestStatus::Skipped));
    assert_eq!(results.get(3).and_then(|r| r.duration_ms), None);
}

#[test]
fn duration_seconds() {
    let output = "  Passed MyTest [1.234 s]";
    let results = parse(output);
    assert_eq!(results.first().and_then(|r| r.duration_ms), Some(1234));
}

#[test]
fn non_status_lines_ignored() {
    let output = "\
  Determining projects to restore...
  Build succeeded.
    Expected: True";

    let results = parse(output);
    assert!(results.is_empty());
}

#[test]
fn empty_output() {
    let results = parse("");
    assert!(results.is_empty());
}
