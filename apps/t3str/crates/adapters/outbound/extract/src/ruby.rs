//! Ruby test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding Ruby test functions.
///
/// Matches three patterns:
/// - `RSpec` `it "description"` blocks (captures the string content as `@name`)
/// - `Rails/ActiveSupport` `test "description"` blocks (captures the string content as `@name`)
/// - `Minitest` `def test_*` methods (captures method name as `@name`)
const QUERY: &str = r#"
(call
  method: (identifier) @fn
  arguments: (argument_list
    (string
      (string_content) @name))
  (#eq? @fn "it"))

(call
  method: (identifier) @fn
  arguments: (argument_list
    (string
      (string_content) @name))
  (#eq? @fn "test"))

(method
  name: (identifier) @name
  (#match? @name "^test_"))
"#;

/// Discover Ruby tests in the given directory.
///
/// Walks the directory for `.rb` files that match test file naming
/// conventions (`*_spec.rb`, `*_test.rb`, `test_*.rb`), then uses
/// tree-sitter to extract `RSpec` `it` blocks, `Rails/ActiveSupport`
/// `test "description"` blocks, and `Minitest` `test_*` methods.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["rb"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Ruby, &lang, QUERY, topic_filter)
}

/// Check if a Ruby file is a test file.
///
/// Returns `true` if the filename ends with `_spec.rb`, `_test.rb`,
/// or starts with `test_`.
fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    stem.ends_with("_spec") || stem.ends_with("_test") || stem.starts_with("test_")
}

#[cfg(test)]
#[path = "ruby_tests.rs"]
mod tests;
