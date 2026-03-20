//! Rust test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding Rust test functions.
///
/// Matches both `#[test]` and `#[tokio::test]` (or any scoped `::test`)
/// attributes immediately followed by a function item.
const QUERY: &str = r#"
(
  (attribute_item
    (attribute
      (identifier) @attr))
  .
  (function_item
    name: (identifier) @name)
  (#eq? @attr "test")
)
(
  (attribute_item
    (attribute
      (scoped_identifier
        path: (identifier) @_ns
        name: (identifier) @attr)))
  .
  (function_item
    name: (identifier) @name)
  (#eq? @attr "test")
)
"#;

/// Discover Rust tests in the given directory.
///
/// Walks the directory for `.rs` files that are test files (in a `tests/`
/// directory, with `test` in the filename, or in a `src/` directory), then
/// uses tree-sitter to extract functions annotated with `#[test]` or
/// `#[tokio::test]`. Files in `src/` are included to catch inline
/// `#[cfg(test)]` modules; non-test files simply return zero matches.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["rs"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Rust, &lang, QUERY, topic_filter)
}

/// Check if a Rust file is a test file.
///
/// Returns `true` if the file is inside a `tests/` directory, inside a
/// `src/` directory (to catch inline `#[cfg(test)]` modules), or has `test`
/// in its filename.
fn is_test_file(path: &Path) -> bool {
    let in_tests_dir = path
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| s == "tests"));

    let in_src_dir = path
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| s == "src"));

    let has_test_in_name = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| name.contains("test"));

    in_tests_dir || in_src_dir || has_test_in_name
}

#[cfg(test)]
#[path = "rust_lang_tests.rs"]
mod tests;
