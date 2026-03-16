//! Single-pass template filler with injection protection.
//!
//! All replacements (simple and content) are applied in a single pass so that
//! no replacement value is ever scanned for placeholders, preventing template
//! injection regardless of value contents.

use std::path::Path;

use crate::error::PipelineError;

/// Single-pass template filler.
///
/// Use [`set`](Self::set) for short, safe values (names, counts) and
/// [`set_content`](Self::set_content) for large blobs that might contain
/// placeholder-like strings. Both are applied in a single pass: all
/// placeholder positions are located first, then replaced simultaneously
/// so no replacement value can inject into another.
pub struct TemplateFiller {
    replacements: Vec<(String, String)>,
    content_replacements: Vec<(String, String)>,
}

impl TemplateFiller {
    /// Create a new empty template filler.
    pub const fn new() -> Self {
        Self {
            replacements: Vec::new(),
            content_replacements: Vec::new(),
        }
    }

    /// Load a template from a file path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    pub fn from_file(path: &Path) -> Result<String, PipelineError> {
        std::fs::read_to_string(path).map_err(|e| {
            PipelineError::Template(format!(
                "failed to read template {}: {e}",
                path.display()
            ))
        })
    }

    /// Add a simple replacement (short strings like names, counts).
    #[must_use]
    pub fn set(mut self, key: &str, value: &str) -> Self {
        self.replacements
            .push((String::from(key), String::from(value)));
        self
    }

    /// Add a content replacement (large blobs like architecture docs, test cases).
    #[must_use]
    pub fn set_content(mut self, key: &str, value: &str) -> Self {
        self.content_replacements
            .push((String::from(key), String::from(value)));
        self
    }

    /// Apply all replacements and return the filled template.
    ///
    /// All replacements (simple and content) are applied in a single pass:
    /// all placeholder positions are located first, then replaced from end
    /// to start so that byte offsets remain valid and no replacement value
    /// is scanned by subsequent replacements. This prevents injection even
    /// if a simple replacement value contains another placeholder string.
    pub fn fill(&self, template: &str) -> String {
        // Combine simple + content replacements into a single list.
        let all: Vec<(String, String)> = self
            .replacements
            .iter()
            .chain(self.content_replacements.iter())
            .cloned()
            .collect();

        if all.is_empty() {
            return String::from(template);
        }

        single_pass_replace(template, &all)
    }
}

/// Holds one match: the byte offset in the haystack, the length of the key,
/// and the index into the replacement list.
struct Match {
    start: usize,
    key_len: usize,
    replacement_index: usize,
}

/// Find all occurrences of all content keys, then replace from end to start.
fn single_pass_replace(haystack: &str, replacements: &[(String, String)]) -> String {
    // Collect all match positions.
    let mut matches: Vec<Match> = Vec::new();
    for (idx, (key, _value)) in replacements.iter().enumerate() {
        let key_bytes = key.as_bytes();
        let key_len = key_bytes.len();
        if key_len == 0 {
            continue;
        }
        let mut search_from: usize = 0;
        while let Some(pos) = haystack.get(search_from..).and_then(|s| s.find(key.as_str())) {
            let absolute = search_from.saturating_add(pos);
            matches.push(Match {
                start: absolute,
                key_len,
                replacement_index: idx,
            });
            search_from = absolute.saturating_add(key_len);
        }
    }

    if matches.is_empty() {
        return String::from(haystack);
    }

    // Sort by start position descending so we can replace from end to start.
    matches.sort_by(|a, b| b.start.cmp(&a.start));

    // Remove overlapping matches (keep the one with the earlier start, i.e.
    // the last in the sorted-descending list when two overlap).
    // After sorting descending, walk and drop any match whose range overlaps
    // with the previously kept match (which has a higher start offset).
    let mut deduped: Vec<Match> = Vec::with_capacity(matches.len());
    for m in matches {
        if let Some(prev) = deduped.last() {
            // prev.start > m.start (descending order).
            // m occupies [m.start .. m.start+m.key_len).
            // prev occupies [prev.start .. prev.start+prev.key_len).
            // Overlap if m.start + m.key_len > prev.start.
            if m.start.saturating_add(m.key_len) > prev.start {
                continue; // overlapping — skip the one with the lower start
            }
        }
        deduped.push(m);
    }

    // Replace from end to start so offsets stay valid.
    let mut out = String::from(haystack);
    for m in &deduped {
        let end = m.start.saturating_add(m.key_len);
        if let Some((_key, value)) = replacements.get(m.replacement_index) {
            // Safety: start and end are valid byte offsets discovered by str::find.
            out.replace_range(m.start..end, value);
        }
    }

    out
}

impl Default for TemplateFiller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
    #[allow(clippy::unwrap_used)] // reason: test assertion with tempdir
    fn from_file_reads_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("template.txt");
        std::fs::write(&path, "Hello {{NAME}}").unwrap();

        let content = TemplateFiller::from_file(&path);
        assert!(content.is_ok(), "should read existing file");
        let content = content.unwrap_or_default();
        assert_eq!(
            content, "Hello {{NAME}}",
            "should return file content"
        );
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
        let filler2 = TemplateFiller::new()
            .set("{{X}}", "ex")
            .set("{{Y}}", "why");
        let result2 = filler2.fill("{{X}} and {{Y}}");
        assert_eq!(
            result2, "ex and why",
            "non-overlapping keys must both be replaced"
        );

        // Adjacent keys (not overlapping): {{A}} immediately followed by {{B}}.
        let filler3 = TemplateFiller::new()
            .set("{{A}}", "1")
            .set("{{B}}", "2");
        let result3 = filler3.fill("{{A}}{{B}}");
        assert_eq!(
            result3, "12",
            "adjacent non-overlapping keys must both be replaced"
        );

        // Overlap where one key's end touches another's start (boundary case for > vs >=).
        // "{{A}}" at pos 0 ends at pos 5. "{{B}}" at pos 5 starts at pos 5.
        // 0 + 5 > 5 is false, so no overlap — both should be replaced.
        // If mutated to >=, 0 + 5 >= 5 is true, incorrectly skipping {{B}}.
        let filler4 = TemplateFiller::new()
            .set("{{A}}", "1")
            .set("{{B}}", "2");
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
    #[allow(clippy::unwrap_used)] // reason: test assertions with tempdir
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
}
