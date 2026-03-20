//! Shared helpers for tree-sitter test discovery.

use std::path::{Path, PathBuf};

use streaming_iterator::StreamingIterator;
use t3str_domain_types::{Language, T3strError, TestFile};

/// Predicate for testing whether a file is a test file.
pub type IsTestFile = dyn Fn(&Path) -> bool;

/// Result of finding test files in a directory.
type FindResult = Result<Vec<PathBuf>, T3strError>;

/// Result of extracting function names from source code.
type ExtractResult = Result<Vec<String>, T3strError>;

/// Filtered function names paired with a relevance flag.
type FilteredNames = (Vec<String>, bool);

/// Directories to skip during file walking.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "target",
    "__pycache__",
    ".venv",
    "build",
    "dist",
    ".bundle",
    ".cargo-home",
    ".gopath",
    "deps",
    "_build",
];

/// Walk `repo_dir` recursively, returning files that match `extensions` and
/// pass the `is_test_file` predicate. Skips common non-source directories.
pub fn find_test_files(
    repo_dir: &Path,
    extensions: &[&str],
    is_test_file: &IsTestFile,
) -> FindResult {
    let mut result = Vec::new();
    let mut stack = vec![repo_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = crate::fs::read_dir(&dir)?;
        for entry in entries {
            let entry = entry.map_err(T3strError::Io)?;
            let path = entry.path();

            if path.is_dir() {
                if should_skip_dir(&path) {
                    continue;
                }
                stack.push(path);
            } else if matches_extension(&path, extensions) && is_test_file(&path) {
                result.push(path);
            }
        }
    }

    result.sort();
    Ok(result)
}

/// Check if a directory should be skipped during walking.
fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| SKIP_DIRS.contains(&name) || name.starts_with('.'))
}

/// Check if a path has one of the given extensions.
fn matches_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| extensions.contains(&ext))
}

/// Parse source code with tree-sitter and extract function names via a query.
///
/// The query must capture test function names with `@name`. All other captures
/// are ignored. Returns the list of captured names.
pub fn extract_with_query(
    source: &str,
    ts_language: &tree_sitter::Language,
    query_str: &str,
) -> ExtractResult {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(ts_language)
        .map_err(|e| T3strError::ParseFailed {
            format: String::from("tree-sitter"),
            reason: e.to_string(),
        })?;

    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| T3strError::ParseFailed {
            format: String::from("tree-sitter"),
            reason: String::from("parse returned None"),
        })?;

    let query =
        tree_sitter::Query::new(ts_language, query_str).map_err(|e| T3strError::ParseFailed {
            format: String::from("tree-sitter-query"),
            reason: e.to_string(),
        })?;

    let name_idx = find_capture_index(&query, "name");
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut names: Vec<String> = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if Some(capture.index) == name_idx {
                if let Some(text) = source.get(capture.node.byte_range()) {
                    if !text.is_empty() {
                        names.push(text.to_owned());
                    }
                }
            }
        }
    }

    Ok(names)
}

/// Find the index of a named capture in a query, if present.
fn find_capture_index(query: &tree_sitter::Query, name: &str) -> Option<u32> {
    query
        .capture_names()
        .iter()
        .position(|n| *n == name)
        .and_then(|i| u32::try_from(i).ok())
}

/// Build `Vec<TestFile>` from discovered files and their test function names.
///
/// Reads each file, extracts test names via tree-sitter query, applies the
/// topic filter, and returns files that contain at least one test function.
/// The topic filter matches against both file paths (case-insensitive) and
/// function names — if a file's path contains the topic, all tests in that
/// file are considered relevant.
pub fn discover_in_files(
    files: &[PathBuf],
    language: Language,
    ts_language: &tree_sitter::Language,
    query_str: &str,
    topic_filter: Option<&str>,
) -> crate::DiscoverResult {
    let mut results = Vec::new();

    for path in files {
        let source = crate::fs::read_to_string(path)?;
        let functions = extract_with_query(&source, ts_language, query_str)?;

        if functions.is_empty() {
            continue;
        }

        // Check if the file path itself matches the topic filter.
        let path_matches = topic_filter.is_some_and(|filter| {
            let filter_lower = filter.to_lowercase();
            let path_str = path.to_string_lossy().to_lowercase();
            path_str.contains(&filter_lower)
        });

        let (filtered, relevant) = apply_filter(&functions, topic_filter, path_matches);

        if !filtered.is_empty() {
            results.push(TestFile {
                path: path.clone(),
                language,
                functions: filtered,
                relevant,
            });
        }
    }

    Ok(results)
}

/// Filter function names by topic, returning `(filtered_names, is_relevant)`.
///
/// If no filter is provided, all names are returned and `relevant` is `true`.
/// With a filter, only names containing the filter substring (case-insensitive)
/// match. If `path_matches` is true (the file path already matched the topic),
/// all functions are returned as relevant.
fn apply_filter(
    functions: &[String],
    topic_filter: Option<&str>,
    path_matches: bool,
) -> FilteredNames {
    match topic_filter {
        None => (functions.to_vec(), true),
        Some(_) if path_matches => {
            // File path matched the topic — all tests in this file are relevant.
            (functions.to_vec(), true)
        }
        Some(filter) => {
            let filter_lower = filter.to_lowercase();
            let matched: Vec<String> = functions
                .iter()
                .filter(|name| name.to_lowercase().contains(&filter_lower))
                .cloned()
                .collect();
            let relevant = !matched.is_empty();
            (matched, relevant)
        }
    }
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;
