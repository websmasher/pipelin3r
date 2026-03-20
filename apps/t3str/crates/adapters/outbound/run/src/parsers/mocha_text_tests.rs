use super::*;

#[test]
fn basic_mocha_output() {
    let output = "\
  Authentication
    \u{2713} should login successfully (45ms)
    \u{2713} should reject bad password
    1) should handle timeout

  2 passing (150ms)
  1 failing

  1) Authentication
       should handle timeout:
     Error: Timeout of 2000ms exceeded";

    let results = parse(output);

    // Two passing, two failing references (inline "1)" + summary "1)")
    // The inline "1) should handle timeout" and the summary "1) Authentication"
    assert!(results.len() >= 3);

    // First: passing with duration
    assert_eq!(results.first().map(|r| r.status), Some(TestStatus::Passed));
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("should login successfully")
    );
    assert_eq!(results.first().and_then(|r| r.duration_ms), Some(45));

    // Second: passing without duration
    assert_eq!(results.get(1).map(|r| r.status), Some(TestStatus::Passed));
    assert_eq!(results.get(1).and_then(|r| r.duration_ms), None);
}

#[test]
fn unicode_variants() {
    let output = "\
    \u{2713} test with check
    \u{2714} test with heavy check
    \u{221A} test with sqrt";

    let results = parse(output);
    assert_eq!(results.len(), 3);
    for r in &results {
        assert_eq!(r.status, TestStatus::Passed);
    }
}

#[test]
fn duration_parsing() {
    let output = "    \u{2713} fast test (2ms)";
    let results = parse(output);
    assert_eq!(results.first().and_then(|r| r.duration_ms), Some(2));
}

#[test]
fn empty_output() {
    let results = parse("");
    assert!(results.is_empty());
}
