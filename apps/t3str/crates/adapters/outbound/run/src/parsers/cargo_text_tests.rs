use super::*;

#[test]
fn basic_output() {
    let output = "\
running 3 tests
test parse_basic ... ok
test parse_complex ... ok
test parse_invalid ... FAILED
test parse_ignored ... ignored

test result: FAILED. 2 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out";

    let results = parse(output);
    assert_eq!(results.len(), 4);

    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("parse_basic")
    );
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Passed));

    assert_eq!(
        results.get(1).map(|r| r.name.as_str()),
        Some("parse_complex")
    );
    assert_eq!(results.get(1).map(|r| r.status), Some(TestStatus::Passed));

    assert_eq!(
        results.get(2).map(|r| r.name.as_str()),
        Some("parse_invalid")
    );
    assert_eq!(results.get(2).map(|r| r.status), Some(TestStatus::Failed));

    assert_eq!(
        results.get(3).map(|r| r.name.as_str()),
        Some("parse_ignored")
    );
    assert_eq!(results.get(3).map(|r| r.status), Some(TestStatus::Skipped));
}

#[test]
fn empty_output() {
    let results = parse("");
    assert!(results.is_empty());
}

#[test]
fn lines_without_test_prefix_ignored() {
    let output = "\
running 1 test
some random line
test my_test ... ok
another random line";

    let results = parse(output);
    assert_eq!(results.len(), 1);
    assert_eq!(results.first().map(|r| r.name.as_str()), Some("my_test"));
}

#[test]
fn lines_without_separator_ignored() {
    let output = "\
test result: ok. 0 passed;
test something_without_separator";

    let results = parse(output);
    assert!(results.is_empty());
}

#[test]
fn duration_is_none() {
    let output = "test my_test ... ok";
    let results = parse(output);
    assert_eq!(results.first().and_then(|r| r.duration_ms), None);
}
