//! Python test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding Python test functions.
///
/// Matches any function definition (top-level or inside a class) whose name
/// starts with `test_`. This covers both pytest-style functions and
/// `unittest.TestCase` methods.
const QUERY: &str = r#"
(function_definition
  name: (identifier) @name
  (#match? @name "^test_"))
"#;

/// Discover Python tests in the given directory.
///
/// Walks the directory for `.py` files matching pytest naming conventions
/// (`test_*.py` or `*_test.py`), then uses tree-sitter to extract functions
/// whose names start with `test_`.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["py"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Python, &lang, QUERY, topic_filter)
}

/// Check if a Python file is a test file.
///
/// Returns `true` if the file is named `test.py`, or if the filename starts
/// with `test_` or ends with `_test` (before the `.py` extension), following
/// pytest conventions.
fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    stem == "test" || stem.starts_with("test_") || stem.ends_with("_test")
}

#[cfg(test)]
#[path = "python_tests.rs"]
mod tests;
