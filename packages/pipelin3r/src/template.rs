//! Two-phase template filler with injection protection.
//!
//! Simple replacements (names, counts) are applied first, then content
//! replacements (large blobs) are applied in a single pass to prevent
//! template injection if their content contains `{{PLACEHOLDER}}` strings.

/// Two-phase template filler.
///
/// Use [`set`](Self::set) for short, safe values (applied first) and
/// [`set_content`](Self::set_content) for large blobs that might contain
/// placeholder-like strings (applied last, in a single pass so they
/// cannot inject into each other).
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

    /// Add a simple replacement (short strings like names, counts).
    /// These are applied first.
    pub fn set(&mut self, key: &str, value: &str) -> &mut Self {
        self.replacements
            .push((String::from(key), String::from(value)));
        self
    }

    /// Add a content replacement (large blobs like architecture docs, test cases).
    /// These are applied in a single pass to prevent template injection if their
    /// content contains `{{PLACEHOLDER}}` strings.
    pub fn set_content(&mut self, key: &str, value: &str) -> &mut Self {
        self.content_replacements
            .push((String::from(key), String::from(value)));
        self
    }

    /// Apply all replacements and return the filled template.
    ///
    /// Simple replacements are applied first (sequentially). Content
    /// replacements are then applied in a single pass: all placeholder
    /// positions are located first, then replaced from end to start so that
    /// byte offsets remain valid and no replacement value is scanned by
    /// subsequent replacements.
    pub fn fill(&self, template: &str) -> String {
        let mut result = String::from(template);

        // Phase 1: simple replacements (sequential is fine — values are short/safe).
        for (key, value) in &self.replacements {
            result = result.replace(key.as_str(), value.as_str());
        }

        // Phase 2: content replacements — single-pass, reverse-offset.
        if !self.content_replacements.is_empty() {
            result = single_pass_replace(&result, &self.content_replacements);
        }

        result
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
        let mut filler = TemplateFiller::new();
        let _ = filler
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
        let mut filler = TemplateFiller::new();
        let _ = filler.set("{{FILENAME}}", "parser.rs");
        let _ = filler.set_content(
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
        let mut filler = TemplateFiller::new();
        let _ = filler
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
        let mut filler = TemplateFiller::new();
        let _ = filler.set_content("{{X}}", "replaced");

        let template = "a {{X}} b {{X}} c";
        let result = filler.fill(template);

        assert_eq!(
            result, "a replaced b replaced c",
            "all occurrences of the same content key should be replaced"
        );
    }
}
