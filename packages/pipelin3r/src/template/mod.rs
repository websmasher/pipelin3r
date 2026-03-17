//! Single-pass template filler with injection protection.
//!
//! All replacements (simple and content) are applied in a single pass so that
//! no replacement value is ever scanned for placeholders, preventing template
//! injection regardless of value contents.

use std::path::Path;

use crate::error::PipelineError;

/// Type alias for key-value replacement pairs.
type ReplacementPair = (String, String);

/// Single-pass template filler.
///
/// Use [`set`](Self::set) for short, safe values (names, counts) and
/// [`set_content`](Self::set_content) for large blobs that might contain
/// placeholder-like strings. Both are applied in a single pass: all
/// placeholder positions are located first, then replaced simultaneously
/// so no replacement value can inject into another.
pub struct TemplateFiller {
    replacements: Vec<ReplacementPair>,
    content_replacements: Vec<ReplacementPair>,
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
        crate::fs::read_to_string(path).map_err(|e| {
            PipelineError::Template(format!("failed to read template {}: {e}", path.display()))
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
        let all: Vec<ReplacementPair> = self
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
fn single_pass_replace(haystack: &str, replacements: &[ReplacementPair]) -> String {
    // Collect all match positions.
    let mut matches: Vec<Match> = Vec::new();
    for (idx, (key, _value)) in replacements.iter().enumerate() {
        let key_bytes = key.as_bytes();
        let key_len = key_bytes.len();
        if key_len == 0 {
            continue;
        }
        let mut search_from: usize = 0;
        while let Some(pos) = haystack
            .get(search_from..)
            .and_then(|s| s.find(key.as_str()))
        {
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
mod tests;
