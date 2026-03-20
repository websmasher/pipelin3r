//! Elixir test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding Elixir test blocks.
///
/// Matches `test "description" do ... end` blocks, capturing the
/// quoted string content as the test name.
const QUERY: &str = r#"
(call
  target: (identifier) @fn
  (arguments
    (string
      (quoted_content) @name))
  (#eq? @fn "test"))
"#;

/// Discover Elixir tests in the given directory.
///
/// Walks the directory for `.exs` files that match the test file naming
/// convention (`*_test.exs`), then uses tree-sitter to extract
/// `test "name" do ... end` blocks.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["exs"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Elixir, &lang, QUERY, topic_filter)
}

/// Check if an Elixir file is a test file.
///
/// Returns `true` if the filename ends with `_test.exs`.
fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let ext = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    stem.ends_with("_test") && ext == "exs"
}

#[cfg(test)]
#[path = "elixir_tests.rs"]
mod tests;
