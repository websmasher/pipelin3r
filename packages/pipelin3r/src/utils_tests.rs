#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

// ── strip_code_fences ──

#[test]
fn strip_fences_with_language_tag() {
    let input = "```json\n{\"a\": 1}\n```";
    assert_eq!(strip_code_fences(input), "{\"a\": 1}");
}

#[test]
fn strip_fences_without_language_tag() {
    let input = "```\nhello world\n```";
    assert_eq!(strip_code_fences(input), "hello world");
}

#[test]
fn strip_fences_no_fences() {
    let input = "just plain text";
    assert_eq!(strip_code_fences(input), "just plain text");
}

#[test]
fn strip_fences_only_opening() {
    let input = "```json\n{\"a\": 1}";
    assert_eq!(strip_code_fences(input), "```json\n{\"a\": 1}");
}

#[test]
fn strip_fences_nested() {
    // Inner fences should be preserved
    let input = "```markdown\nsome text\n```rust\nfn main() {}\n```\nmore text\n```";
    let result = strip_code_fences(input);
    assert!(
        result.contains("```rust"),
        "inner fences should be preserved"
    );
}

#[test]
fn strip_fences_empty_content() {
    let input = "```\n```";
    assert_eq!(strip_code_fences(input), "");
}

#[test]
fn strip_fences_with_surrounding_whitespace() {
    let input = "  ```json\n{\"a\": 1}\n```  ";
    assert_eq!(strip_code_fences(input), "{\"a\": 1}");
}

// ── strip_preamble ──

#[test]
fn strip_preamble_basic() {
    let text = "Here is the result:\n{\"key\": \"value\"}";
    assert_eq!(strip_preamble(text, &["{"]), "{\"key\": \"value\"}");
}

#[test]
fn strip_preamble_multiple_markers() {
    let text = "Preamble text\n---\nactual content";
    assert_eq!(strip_preamble(text, &["{", "---"]), "---\nactual content");
}

#[test]
fn strip_preamble_earliest_marker_wins() {
    let text = "start {json} ---divider";
    assert_eq!(strip_preamble(text, &["---", "{"]), "{json} ---divider");
}

#[test]
fn strip_preamble_no_marker_found() {
    let text = "no markers here";
    assert_eq!(strip_preamble(text, &["{", "---"]), "no markers here");
}

#[test]
fn strip_preamble_empty_markers() {
    let text = "some text";
    assert_eq!(strip_preamble(text, &[]), "some text");
}

// ── parse_labeled_fields ──

#[test]
fn parse_fields_basic() {
    let text = "SCENE: A dark forest\nCAPTION: The hero enters";
    let fields = parse_labeled_fields(text, &["SCENE:", "CAPTION:"]);
    assert_eq!(fields.get("SCENE"), Some(&"A dark forest"));
    assert_eq!(fields.get("CAPTION"), Some(&"The hero enters"));
}

#[test]
fn parse_fields_multiline_value() {
    let text = "SCENE: A dark forest\nwith tall trees\nCAPTION: Short";
    let fields = parse_labeled_fields(text, &["SCENE:", "CAPTION:"]);
    assert_eq!(fields.get("SCENE"), Some(&"A dark forest\nwith tall trees"));
    assert_eq!(fields.get("CAPTION"), Some(&"Short"));
}

#[test]
fn parse_fields_missing_labels() {
    let text = "SCENE: A dark forest";
    let fields = parse_labeled_fields(text, &["SCENE:", "CAPTION:", "ALT:"]);
    assert_eq!(fields.len(), 1);
    assert_eq!(fields.get("SCENE"), Some(&"A dark forest"));
}

#[test]
fn parse_fields_no_matches() {
    let text = "no labels here";
    let fields = parse_labeled_fields(text, &["SCENE:", "CAPTION:"]);
    assert!(fields.is_empty());
}

#[test]
fn parse_fields_label_mid_line_ignored() {
    // Labels not at line start should be ignored
    let text = "This has SCENE: in the middle\nSCENE: actual value";
    let fields = parse_labeled_fields(text, &["SCENE:"]);
    assert_eq!(fields.len(), 1);
    assert_eq!(fields.get("SCENE"), Some(&"actual value"));
}

// ── chunk_by_size ──

#[test]
fn chunk_basic() {
    let items = vec!["aa", "bbb", "c", "dddd"];
    let chunks = chunk_by_size(items, 5, |s| s.len());
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks.first().map(Vec::len), Some(2)); // "aa" + "bbb" = 5
    assert_eq!(chunks.get(1).map(Vec::len), Some(2)); // "c" + "dddd" = 5
}

#[test]
fn chunk_single_item_exceeds_max() {
    let items = vec!["very-long-string"];
    let chunks = chunk_by_size(items, 3, |s| s.len());
    assert_eq!(chunks.len(), 1, "oversized item gets its own chunk");
    assert_eq!(
        chunks.first().and_then(|c| c.first()),
        Some(&"very-long-string")
    );
}

#[test]
fn chunk_empty_input() {
    let items: Vec<&str> = vec![];
    let chunks = chunk_by_size(items, 10, |s| s.len());
    assert!(chunks.is_empty());
}

#[test]
fn chunk_exact_fit() {
    let items = vec![1, 2, 3, 4, 5];
    // Each item size = item value, max = 6
    // 1+2+3 = 6 → first chunk
    // 4 → second chunk (4 alone)
    // 5 → third chunk (5 alone)
    let chunks = chunk_by_size(items, 6, |&n| n);
    assert_eq!(chunks.len(), 3);
}

#[test]
fn chunk_all_fit_in_one() {
    let items = vec!["a", "b", "c"];
    let chunks = chunk_by_size(items, 100, |s| s.len());
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks.first().map(Vec::len), Some(3));
}

#[test]
fn chunk_each_exceeds_max() {
    let items = vec!["aaaa", "bbbb", "cccc"];
    let chunks = chunk_by_size(items, 2, |s| s.len());
    assert_eq!(chunks.len(), 3, "each oversized item in its own chunk");
}

#[test]
fn chunk_zero_max_size() {
    let items = vec!["a", "b"];
    let chunks = chunk_by_size(items, 0, |s| s.len());
    // max_size=0 is treated as 1, so each item gets its own chunk
    assert_eq!(chunks.len(), 2);
}
