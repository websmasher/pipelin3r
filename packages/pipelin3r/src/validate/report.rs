//! Validation report types for the validate-and-fix loop.

/// A single finding from a validation run.
#[derive(Debug, Clone)]
pub struct ValidationFinding {
    /// Category tag for grouping (e.g., `"lint"`, `"type-error"`, `"test-fail"`).
    pub tag: String,
    /// Human-readable description of the finding.
    pub message: String,
    /// Optional key identifying the specific item that failed (e.g., file path, test name).
    pub item_key: Option<String>,
}

impl ValidationFinding {
    /// Create a new finding with a tag and message.
    #[must_use]
    pub fn new(tag: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            message: message.into(),
            item_key: None,
        }
    }

    /// Create a new finding with a tag, message, and item key.
    #[must_use]
    pub fn with_key(
        tag: impl Into<String>,
        message: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            tag: tag.into(),
            message: message.into(),
            item_key: Some(key.into()),
        }
    }
}

/// Result of a single validation run.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Whether all checks passed.
    pub passed: bool,
    /// Structured findings (empty when `passed` is `true`).
    pub findings: Vec<ValidationFinding>,
    /// Optional raw output from the validation tool (e.g., compiler stderr).
    pub raw_output: Option<String>,
}

impl ValidationReport {
    /// Create a passing report with no findings.
    #[must_use]
    pub const fn pass() -> Self {
        Self {
            passed: true,
            findings: Vec::new(),
            raw_output: None,
        }
    }

    /// Create a failing report from raw output text (no structured findings).
    #[must_use]
    pub fn fail_raw(output: &str) -> Self {
        Self {
            passed: false,
            findings: Vec::new(),
            raw_output: Some(String::from(output)),
        }
    }

    /// Create a failing report from structured findings.
    #[must_use]
    pub const fn fail(findings: Vec<ValidationFinding>) -> Self {
        Self {
            passed: false,
            findings,
            raw_output: None,
        }
    }

    /// Return all findings matching the given tag.
    #[must_use]
    pub fn findings_with_tag<'a>(&'a self, tag: &str) -> Vec<&'a ValidationFinding> {
        self.findings.iter().filter(|f| f.tag == tag).collect()
    }

    /// Render the report as a human-readable markdown string.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();

        if self.passed {
            out.push_str("## Validation: PASSED\n\nAll checks passed.\n");
            return out;
        }

        out.push_str("## Validation: FAILED\n\n");

        if !self.findings.is_empty() {
            out.push_str("### Findings\n\n");
            for finding in &self.findings {
                out.push_str("- **[");
                out.push_str(&finding.tag);
                out.push_str("]** ");
                if let Some(ref key) = finding.item_key {
                    out.push('`');
                    out.push_str(key);
                    out.push_str("`: ");
                }
                out.push_str(&finding.message);
                out.push('\n');
            }
            out.push('\n');
        }

        if let Some(ref raw) = self.raw_output {
            out.push_str("### Raw Output\n\n```\n");
            out.push_str(raw);
            out.push_str("\n```\n");
        }

        out
    }
}
