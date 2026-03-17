//! Utility functions for processing LLM output.
//!
//! These helpers handle common patterns when working with LLM-generated text:
//! stripping code fences, removing preamble text, parsing labeled fields, and
//! chunking items by estimated size.

use std::collections::BTreeMap;

/// Strip outermost code fences from text.
///
/// If the text starts with a line beginning with `` ``` `` (optionally followed
/// by a language tag) and ends with a line that is exactly `` ``` ``, the
/// fences and the content between them is returned without the fence lines.
/// Otherwise the original text is returned unchanged.
///
/// # Examples
///
/// ```
/// use pipelin3r::utils::strip_code_fences;
///
/// let fenced = "```json\n{\"a\": 1}\n```";
/// assert_eq!(strip_code_fences(fenced), "{\"a\": 1}");
///
/// let plain = "no fences here";
/// assert_eq!(strip_code_fences(plain), "no fences here");
/// ```
#[must_use]
pub fn strip_code_fences(text: &str) -> &str {
    let trimmed = text.trim();

    // Must start with ``` (optionally followed by language tag)
    if !trimmed.starts_with("```") {
        return text;
    }

    // Must end with ``` on its own line
    if !trimmed.ends_with("```") {
        return text;
    }

    // Find the end of the opening fence line
    let Some(first_newline) = trimmed.find('\n') else {
        // Single line like "``````" — not a valid fenced block
        return text;
    };

    // The closing fence must be separate from the opening fence
    let Some(inner) = trimmed.get(first_newline..) else {
        return text;
    };
    let closing_start = inner.rfind("```");

    // Make sure the closing ``` is on its own line (preceded by newline or is at start)
    match closing_start {
        Some(0) => {
            // No content between fences
            ""
        }
        Some(pos) => {
            let before_closing = inner.get(..pos).unwrap_or("");
            // Strip leading newline and trailing newline before closing fence
            let content = before_closing.trim_matches('\n');
            if content.is_empty() { "" } else { content }
        }
        None => text,
    }
}

/// Strip preamble text before the first occurrence of any marker.
///
/// Scans the text for the first occurrence of any of the given marker strings.
/// Returns the text starting from that marker. If no marker is found, returns
/// the original text unchanged.
///
/// This is useful for stripping conversational preamble from LLM output, e.g.
/// "Here is the JSON you requested:\n{...}" with markers `["{"]` returns `"{...}"`.
///
/// # Examples
///
/// ```
/// use pipelin3r::utils::strip_preamble;
///
/// let text = "Sure! Here it is:\n{\"key\": \"value\"}";
/// assert_eq!(strip_preamble(text, &["{"]), "{\"key\": \"value\"}");
/// ```
#[must_use]
pub fn strip_preamble<'a>(text: &'a str, markers: &[&str]) -> &'a str {
    let mut earliest: Option<usize> = None;

    for marker in markers {
        if let Some(pos) = text.find(marker) {
            earliest = Some(match earliest {
                Some(current) => {
                    if pos < current {
                        pos
                    } else {
                        current
                    }
                }
                None => pos,
            });
        }
    }

    match earliest {
        Some(pos) => text.get(pos..).unwrap_or(text),
        None => text,
    }
}

/// Parse labeled fields from LLM output.
///
/// Searches the text for lines starting with any of the given labels (e.g.
/// `"SCENE:"`, `"CAPTION:"`). For each label found, captures all text from
/// after the label until the next label or end of text. The returned map uses
/// the label (without the trailing colon) as key.
///
/// Labels should include the trailing colon, e.g. `["SCENE:", "CAPTION:"]`.
///
/// # Examples
///
/// ```
/// use pipelin3r::utils::parse_labeled_fields;
///
/// let text = "SCENE: A dark forest\nCAPTION: The hero enters";
/// let fields = parse_labeled_fields(text, &["SCENE:", "CAPTION:"]);
/// assert_eq!(fields.get("SCENE"), Some(&"A dark forest"));
/// assert_eq!(fields.get("CAPTION"), Some(&"The hero enters"));
/// ```
#[must_use]
pub fn parse_labeled_fields<'a>(text: &'a str, labels: &[&'a str]) -> BTreeMap<&'a str, &'a str> {
    let mut result = BTreeMap::new();

    // Find all label positions
    #[allow(
        clippy::type_complexity,
        reason = "local collection of position-label pairs"
    )]
    let mut positions: Vec<(usize, &str)> = Vec::new();

    for label in labels {
        let mut search_from: usize = 0;
        while let Some(pos) = text.get(search_from..).and_then(|s| s.find(label)) {
            let abs_pos = search_from.saturating_add(pos);
            // Only match at start of line or start of text
            if abs_pos == 0
                || text
                    .as_bytes()
                    .get(abs_pos.wrapping_sub(1))
                    .is_some_and(|&b| b == b'\n')
            {
                positions.push((abs_pos, label));
            }
            search_from = abs_pos.saturating_add(label.len());
        }
    }

    // Sort by position
    positions.sort_by_key(|&(pos, _)| pos);

    // Extract values between consecutive labels
    for (i, &(pos, label)) in positions.iter().enumerate() {
        let value_start = pos.saturating_add(label.len());
        let value_end = positions
            .get(i.saturating_add(1))
            .map_or(text.len(), |&(next_pos, _)| next_pos);

        let value = text.get(value_start..value_end).unwrap_or("").trim();

        // Strip trailing colon from label for the key
        let key = label.strip_suffix(':').unwrap_or(label).trim();

        let _ = result.insert(key, value);
    }

    result
}

/// Split items into chunks where each chunk's estimated size is under `max_size`.
///
/// Items are added to the current chunk until adding the next item would exceed
/// `max_size`. At that point a new chunk is started. An item that exceeds
/// `max_size` on its own is placed in a single-element chunk (never dropped).
///
/// # Examples
///
/// ```
/// use pipelin3r::utils::chunk_by_size;
///
/// let items = vec!["aa", "bbb", "c", "dddd"];
/// let chunks = chunk_by_size(items, 5, |s| s.len());
/// // "aa" (2) + "bbb" (3) = 5 → first chunk
/// // "c" (1) + "dddd" (4) = 5 → second chunk
/// assert_eq!(chunks.len(), 2);
/// ```
#[must_use]
pub fn chunk_by_size<T>(
    items: Vec<T>,
    max_size: usize,
    size_fn: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> {
    if items.is_empty() {
        return Vec::new();
    }

    let effective_max = if max_size == 0 { 1 } else { max_size };
    let mut chunks: Vec<Vec<T>> = Vec::new();
    let mut current_chunk: Vec<T> = Vec::new();
    let mut current_size: usize = 0;

    for item in items {
        let item_size = size_fn(&item);

        // If adding this item would exceed max and chunk is non-empty, start new chunk
        if !current_chunk.is_empty() && current_size.saturating_add(item_size) > effective_max {
            chunks.push(current_chunk);
            current_chunk = Vec::new();
            current_size = 0;
        }

        current_size = current_size.saturating_add(item_size);
        current_chunk.push(item);
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

#[cfg(test)]
#[path = "utils_tests.rs"]
mod tests;
