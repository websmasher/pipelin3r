//! Go test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding Go test and benchmark functions.
///
/// Matches function declarations whose names start with `Test` or `Benchmark`.
/// Go convention requires these functions to accept `*testing.T` or `*testing.B`
/// respectively, but the name prefix is sufficient for discovery since only
/// `_test.go` files are scanned.
const QUERY: &str = r#"
(function_declaration
  name: (identifier) @name
  (#match? @name "^(Test|Benchmark)"))
"#;

/// Discover Go tests in the given directory.
///
/// Walks the directory for `*_test.go` files and uses tree-sitter to extract
/// functions whose names start with `Test` or `Benchmark`.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["go"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Go, &lang, QUERY, topic_filter)
}

/// Check if a Go file is a test file.
///
/// Returns `true` if the filename ends with `_test.go`, following Go convention.
fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| name.ends_with("_test.go"))
}

#[cfg(test)]
#[path = "go_tests.rs"]
mod tests;
