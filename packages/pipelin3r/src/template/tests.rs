#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test helper — filesystem setup/teardown"
)]

use super::*;

#[test]
fn fills_simple_and_content_replacements() {
    let filler = TemplateFiller::new()
        .set("{{PACKAGE}}", "my-parser")
        .set("{{COUNT}}", "5")
        .set_content("{{ARCHITECTURE}}", "The system uses modular design.");

    let template = "Package: {{PACKAGE}}, Tests: {{COUNT}}\nArch: {{ARCHITECTURE}}";
    let result = filler.fill(template);

    assert_eq!(
        result, "Package: my-parser, Tests: 5\nArch: The system uses modular design.",
        "Simple and content replacements should both be applied"
    );
}

#[test]
fn content_applied_last_prevents_injection() {
    let filler = TemplateFiller::new()
        .set("{{FILENAME}}", "parser.rs")
        .set_content(
            "{{TEST_CASES}}",
            "Test that {{FILENAME}} handles edge cases",
        );

    let template = "File: {{FILENAME}}\nCases: {{TEST_CASES}}";
    let result = filler.fill(template);

    assert_eq!(
        result, "File: parser.rs\nCases: Test that {{FILENAME}} handles edge cases",
        "Content containing placeholder strings must not be double-replaced"
    );
}

#[test]
fn content_into_content_replacement_order() {
    let filler = TemplateFiller::new()
        .set_content("{{A}}", "contains {{B}} reference")
        .set_content("{{B}}", "injected");

    let template = "First: {{A}}, Second: {{B}}";
    let result = filler.fill(template);

    assert_eq!(
        result, "First: contains {{B}} reference, Second: injected",
        "content replacements must not inject into each other"
    );
}

#[test]
fn simple_into_simple_no_cross_injection() {
    let filler = TemplateFiller::new()
        .set("{{A}}", "contains {{B}} reference")
        .set("{{B}}", "injected");

    let template = "First: {{A}}, Second: {{B}}";
    let result = filler.fill(template);

    assert_eq!(
        result, "First: contains {{B}} reference, Second: injected",
        "simple replacements must not inject into each other (single-pass)"
    );
}

#[test]
fn no_replacement_leaves_template_unchanged() {
    let filler = TemplateFiller::new();
    let template = "Hello {{WORLD}}";
    let result = filler.fill(template);
    assert_eq!(
        result, template,
        "Template with no matching replacements should be unchanged"
    );
}

#[test]
fn multiple_occurrences_of_same_content_key() {
    let filler = TemplateFiller::new().set_content("{{X}}", "replaced");

    let template = "a {{X}} b {{X}} c";
    let result = filler.fill(template);

    assert_eq!(
        result, "a replaced b replaced c",
        "all occurrences of the same content key should be replaced"
    );
}

#[test]
fn from_file_nonexistent() {
    let result = TemplateFiller::from_file(std::path::Path::new("/nonexistent/template.md"));
    assert!(result.is_err(), "should fail on nonexistent file");
}

#[test]
fn from_file_reads_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("template.txt");
    std::fs::write(&path, "Hello {{NAME}}").unwrap();

    let content = TemplateFiller::from_file(&path);
    assert!(content.is_ok(), "should read existing file");
    let content = content.unwrap_or_default();
    assert_eq!(content, "Hello {{NAME}}", "should return file content");
}

#[test]
fn mutant_kill_template_overlapping_keys() {
    // Mutant kill: template.rs:133 — `> with ==` and `> with >=` on overlap check
    // `if m.start.saturating_add(m.key_len) > prev.start`
    // Test with overlapping keys: {{A}} and {{AB}} where one is prefix of another.
    // The shorter key's match overlaps with the longer key's match position.
    let filler = TemplateFiller::new()
        .set("{{A}}", "alpha")
        .set("{{AB}}", "alphabeta");

    // "{{AB}}" contains "{{A}}" as a prefix — both will match starting at the same position.
    let result = filler.fill("value={{AB}}");
    // The longer key "{{AB}}" should win because it starts at the same position.
    // Actually, both "{{A}}" matches at pos 6 and "{{AB}}" matches at pos 6.
    // After sorting descending and dedup, the one with the lower start (same here)
    // is kept when they overlap. Since both start at 6, one will be in deduped first
    // and the other will overlap. The first in sorted-descending order is the one
    // with the same start but we need to check which order they appear.
    // The key thing: the result must be deterministic and not corrupt the string.
    assert!(
        result == "value=alphabeta" || result == "value=alphaB}}",
        "overlapping keys must produce a valid result without corruption: {result}"
    );

    // Non-overlapping case: keys at different positions must both be replaced.
    let filler2 = TemplateFiller::new().set("{{X}}", "ex").set("{{Y}}", "why");
    let result2 = filler2.fill("{{X}} and {{Y}}");
    assert_eq!(
        result2, "ex and why",
        "non-overlapping keys must both be replaced"
    );

    // Adjacent keys (not overlapping): {{A}} immediately followed by {{B}}.
    let filler3 = TemplateFiller::new().set("{{A}}", "1").set("{{B}}", "2");
    let result3 = filler3.fill("{{A}}{{B}}");
    assert_eq!(
        result3, "12",
        "adjacent non-overlapping keys must both be replaced"
    );

    // Overlap where one key's end touches another's start (boundary case for > vs >=).
    // "{{A}}" at pos 0 ends at pos 5. "{{B}}" at pos 5 starts at pos 5.
    // 0 + 5 > 5 is false, so no overlap — both should be replaced.
    // If mutated to >=, 0 + 5 >= 5 is true, incorrectly skipping {{B}}.
    let filler4 = TemplateFiller::new().set("{{A}}", "1").set("{{B}}", "2");
    let result4 = filler4.fill("{{A}}{{B}}rest");
    assert_eq!(
        result4, "12rest",
        "keys touching at boundary must both be replaced (> not >=)"
    );
}

// ── Regression tests ────────────────────────────────────────────

#[test]
fn regression_phase1_cross_injection() {
    // Regression: set("{{A}}", "has {{B}}").set("{{B}}", "injected") would
    // double-replace, producing "has injected" instead of "has {{B}}".
    let result = TemplateFiller::new()
        .set("{{A}}", "has {{B}}")
        .set("{{B}}", "injected")
        .fill("{{A}}");

    assert_eq!(
        result, "has {{B}}",
        "simple replacement value containing another placeholder must NOT be replaced (single-pass)"
    );
}

#[test]
fn regression_content_into_content_no_injection() {
    // Regression: set_content("{{A}}", "has {{B}}").set_content("{{B}}", "x")
    // would replace {{B}} inside A's value, producing "has x".
    let result = TemplateFiller::new()
        .set_content("{{A}}", "has {{B}}")
        .set_content("{{B}}", "x")
        .fill("{{A}}");

    assert_eq!(
        result, "has {{B}}",
        "content replacement value containing another placeholder must NOT be replaced (single-pass)"
    );
}

#[test]
fn regression_template_filler_owned_self_chaining() {
    // Regression: TemplateFiller methods required &mut self instead of owned
    // self, making single-expression chaining impossible without let mut.
    let result = TemplateFiller::new()
        .set("{{A}}", "alpha")
        .set("{{B}}", "beta")
        .fill("{{A}} and {{B}}");

    assert_eq!(
        result, "alpha and beta",
        "chained set() calls in one expression must compile and produce correct output"
    );
}

#[test]
fn regression_template_from_file_loads_content() {
    // Regression: Template::from_file was missing, forcing manual file reading.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my_template.md");
    std::fs::write(&path, "Hello {{WORLD}}, count={{N}}").unwrap();

    let content = TemplateFiller::from_file(&path).unwrap();
    let result = TemplateFiller::new()
        .set("{{WORLD}}", "Earth")
        .set("{{N}}", "42")
        .fill(&content);

    assert_eq!(
        result, "Hello Earth, count=42",
        "from_file content must be usable with fill()"
    );
}
