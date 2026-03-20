//! JavaScript and TypeScript test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding JavaScript/TypeScript test functions.
///
/// Matches `it("name", ...)` and `test("name", ...)` call expressions,
/// capturing the string argument as the test name. Covers Mocha, Jest,
/// and Vitest conventions.
const QUERY: &str = r#"
(call_expression
  function: (identifier) @fn
  arguments: (arguments
    (string
      (string_fragment) @name))
  (#match? @fn "^(it|test)$"))
"#;

/// Discover JavaScript/TypeScript tests in the given directory.
///
/// Walks the directory for JS/TS files that follow common test file naming
/// conventions (`.test.js`, `.spec.ts`, files in `__tests__/` or `test/`
/// directories), then uses tree-sitter to extract `it()` and `test()` calls.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    let files =
        helpers::find_test_files(repo_dir, &["js", "ts", "jsx", "tsx", "mjs"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Javascript, &lang, QUERY, topic_filter)
}

/// Check if a JavaScript/TypeScript file is a test file.
///
/// Returns `true` if the filename contains `.test.` or `.spec.`, or if
/// the file path contains a `__tests__/` or `/test/` directory.
fn is_test_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let has_test_pattern = file_name.contains(".test.") || file_name.contains(".spec.");

    let in_test_dir = path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|s| s == "__tests__" || s == "test")
    });

    has_test_pattern || in_test_dir
}

#[cfg(test)]
#[path = "javascript_tests.rs"]
mod tests;
