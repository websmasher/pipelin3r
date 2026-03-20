use super::*;

#[test]
fn basic_pass_fail_skip() {
    let input = r#"{"Time":"2024-01-01T00:00:00Z","Action":"pass","Package":"github.com/foo/bar","Test":"TestA","Elapsed":0.001}
{"Time":"2024-01-01T00:00:00Z","Action":"fail","Package":"github.com/foo/bar","Test":"TestB","Elapsed":0.002}
{"Time":"2024-01-01T00:00:00Z","Action":"skip","Package":"github.com/foo/bar","Test":"TestC","Elapsed":0.0}"#;

    let results = parse(input).ok();
    let results = results.as_ref();
    assert!(results.is_some(), "parse should succeed");
    let results = results.map_or(&[][..], Vec::as_slice);
    assert_eq!(results.len(), 3, "should have 3 results");

    assert_eq!(
        results.first().map(|r| r.status),
        Some(TestStatus::Passed),
        "first is pass"
    );
    assert_eq!(
        results.get(1).map(|r| r.status),
        Some(TestStatus::Failed),
        "second is fail"
    );
    assert_eq!(
        results.get(2).map(|r| r.status),
        Some(TestStatus::Skipped),
        "third is skip"
    );
    assert_eq!(
        results.first().map(|r| r.name.as_str()),
        Some("github.com/foo/bar::TestA"),
        "name format"
    );
}

#[test]
fn package_level_events_filtered() {
    let input = r#"{"Time":"2024-01-01T00:00:00Z","Action":"pass","Package":"github.com/foo/bar","Elapsed":0.5}
{"Time":"2024-01-01T00:00:00Z","Action":"pass","Package":"github.com/foo/bar","Test":"TestA","Elapsed":0.001}"#;

    let results = parse(input).ok();
    let results = results.as_ref();
    assert!(results.is_some(), "parse should succeed");
    let results = results.map_or(&[][..], Vec::as_slice);
    assert_eq!(results.len(), 1, "package-level event should be filtered");
}

#[test]
fn deduplication() {
    let input = r#"{"Action":"pass","Package":"pkg","Test":"TestA","Elapsed":0.001}
{"Action":"fail","Package":"pkg","Test":"TestA","Elapsed":0.002}"#;

    let results = parse(input).ok();
    let results = results.as_ref();
    assert!(results.is_some(), "parse should succeed");
    let results = results.map_or(&[][..], Vec::as_slice);
    assert_eq!(results.len(), 1, "should dedup by package+test");
    assert_eq!(
        results.first().map(|r| r.status),
        Some(TestStatus::Passed),
        "keeps first terminal event"
    );
}

#[test]
fn mixed_json_and_non_json() {
    let input = "not json at all\n{\"Action\":\"pass\",\"Package\":\"p\",\"Test\":\"T\",\"Elapsed\":0.1}\nmore garbage";

    let results = parse(input).ok();
    let results = results.as_ref();
    assert!(results.is_some(), "parse should succeed");
    let results = results.map_or(&[][..], Vec::as_slice);
    assert_eq!(results.len(), 1, "should parse valid line only");
}

#[test]
fn empty_input() {
    let results = parse("").ok();
    let results = results.as_ref();
    assert!(results.is_some(), "parse should succeed");
    let results = results.map_or(&[][..], Vec::as_slice);
    assert!(results.is_empty(), "empty input => empty results");
}
