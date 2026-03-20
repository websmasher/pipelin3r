use super::*;

#[test]
fn console_output_with_passes_and_failures() {
    let output = "\
   √ testParseStringEncryption
   √ testParseStringCsaf
   × testParseStringSignedFile

   Failed: true should be false in testParseStringSignedFile()

FAILURES! (3 tests, 1 failure, 2.6 seconds)";

    let results = parse(output);
    assert_eq!(results.len(), 3);

    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("testParseStringEncryption")
    );
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Passed));

    assert_eq!(results.get(1).map(|r| r.status), Some(TestStatus::Passed));
    assert_eq!(
        results.get(1).map(|r| r.name.as_str()),
        Some("testParseStringCsaf")
    );

    assert_eq!(results.get(2).map(|r| r.status), Some(TestStatus::Failed));
    assert_eq!(
        results.get(2).map(|r| r.name.as_str()),
        Some("testParseStringSignedFile")
    );
    assert_eq!(
        results.get(2).and_then(|r| r.message.as_deref()),
        Some("true should be false")
    );
}

#[test]
fn all_passing() {
    let output = "\
   √ testOne
   √ testTwo
   √ testThree

OK (3 tests, 0.5 seconds)";

    let results = parse(output);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.status == TestStatus::Passed));
}

#[test]
fn skipped_with_reason_stripped() {
    let output = "   s testFoo (needs PHP 8.2)";
    let results = parse(output);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("testFoo")
    );
    assert_eq!(
        results.first().map(|r| r.status),
        Some(TestStatus::Skipped)
    );
}

#[test]
fn skipped_without_reason() {
    let output = "   s testBar";
    let results = parse(output);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("testBar")
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
   √ testOne";

    let results = parse(output);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("testOne")
    );
}

#[test]
fn large_suite_with_mixed_results() {
    // Simulates the real-world scenario: 37 tests, 34 pass, 3 fail
    let output = "\
   √ test1
   √ test2
   √ test3
   √ test4
   √ test5
   × testFail1
   √ test6
   × testFail2
   √ test7
   s testSkipped1 (requires extension)
   × testFail3

   Failed: expected true in testFail1()
   Failed: assertion error in testFail2()
   Failed: not equal in testFail3()

FAILURES! (11 tests, 3 failures, 1 skipped, 4.2 seconds)";

    let results = parse(output);
    assert_eq!(results.len(), 11);

    let passed_count = results.iter().filter(|r| r.status == TestStatus::Passed).count();
    let failed_count = results.iter().filter(|r| r.status == TestStatus::Failed).count();
    let skipped_count = results.iter().filter(|r| r.status == TestStatus::Skipped).count();

    assert_eq!(passed_count, 7);
    assert_eq!(failed_count, 3);
    assert_eq!(skipped_count, 1);

    // Check failure messages are attached.
    let fail1 = results.iter().find(|r| r.name == "testFail1");
    assert_eq!(
        fail1.and_then(|r| r.message.as_deref()),
        Some("expected true")
    );
}

#[test]
fn multiple_failures_with_details() {
    let output = "\
   × testA
   × testB

   Failed: value was null in testA()
   Failed: timeout exceeded in testB()";

    let results = parse(output);
    assert_eq!(results.len(), 2);

    assert_eq!(
        results.first().and_then(|r| r.message.as_deref()),
        Some("value was null")
    );
    assert_eq!(
        results.get(1).and_then(|r| r.message.as_deref()),
        Some("timeout exceeded")
    );
}

#[test]
fn failure_detail_without_matching_test_ignored() {
    // A "Failed:" line that doesn't match any test name is harmless.
    let output = "\
   √ testOne

   Failed: something in unknownTest()";

    let results = parse(output);
    assert_eq!(results.len(), 1);
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Passed));
    assert!(results.first().and_then(|r| r.message.as_deref()).is_none());
}
