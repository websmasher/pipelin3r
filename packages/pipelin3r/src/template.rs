//! Two-phase template filler with injection protection.
//!
//! Simple replacements (names, counts) are applied first, then content
//! replacements (large blobs) are applied last to prevent template injection
//! if their content contains `{{PLACEHOLDER}}` strings.

/// Two-phase template filler.
///
/// Use [`set`](Self::set) for short, safe values (applied first) and
/// [`set_content`](Self::set_content) for large blobs that might contain
/// placeholder-like strings (applied last).
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
    /// These are applied last to prevent template injection if their content
    /// contains `{{PLACEHOLDER}}` strings.
    pub fn set_content(&mut self, key: &str, value: &str) -> &mut Self {
        self.content_replacements
            .push((String::from(key), String::from(value)));
        self
    }

    /// Apply all replacements and return the filled template.
    /// Simple replacements are applied first, then content replacements.
    pub fn fill(&self, template: &str) -> String {
        let mut result = String::from(template);

        for (key, value) in &self.replacements {
            result = result.replace(key.as_str(), value.as_str());
        }

        for (key, value) in &self.content_replacements {
            result = result.replace(key.as_str(), value.as_str());
        }

        result
    }
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
            result, "First: contains injected reference, Second: injected",
            "content replacements are applied sequentially — later ones can substitute into earlier ones"
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
}
