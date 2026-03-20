//! PHP test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding PHP test methods.
///
/// Matches methods whose name starts with `test` inside class bodies,
/// following the `PHPUnit` naming convention.
const QUERY: &str = r#"
(method_declaration
  name: (name) @name
  (#match? @name "^test"))
"#;

/// Discover PHP tests in the given directory.
///
/// Walks the directory for `.php` and `.phpt` files that follow `PHPUnit`
/// naming conventions, then uses tree-sitter to extract methods whose
/// names start with `test`.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_php::LANGUAGE_PHP.into();
    let files = helpers::find_test_files(repo_dir, &["php", "phpt"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Php, &lang, QUERY, topic_filter)
}

/// Check if a PHP file is a test file.
///
/// Returns `true` if the filename ends with `Test.php`, or starts with
/// `test` or ends with `Test` and ends with `.phpt`.
fn is_test_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let ext = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    // *Test.php — filename ends with Test.php
    let is_phpunit = ext == "php" && stem.ends_with("Test");

    // test*.phpt — filename starts with test and ends with .phpt
    let is_phpt = ext == "phpt" && (file_name.starts_with("test") || stem.ends_with("Test"));

    is_phpunit || is_phpt
}

#[cfg(test)]
#[path = "php_tests.rs"]
mod tests;
