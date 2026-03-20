//! Java test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding Java test methods.
///
/// Matches methods annotated with `@Test` (`JUnit` 4 and `JUnit` 5).
const QUERY: &str = r#"
(method_declaration
  (modifiers
    (marker_annotation
      name: (identifier) @attr
      (#eq? @attr "Test")))
  name: (identifier) @name)
"#;

/// Discover Java tests in the given directory.
///
/// Walks the directory for `.java` files that match test file naming
/// conventions (`*Test.java`, `*Tests.java`, `Test*.java`), then uses
/// tree-sitter to extract methods annotated with `@Test`.
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["java"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Java, &lang, QUERY, topic_filter)
}

/// Check if a Java file is a test file.
///
/// Returns `true` if the filename ends with `Test.java`, `Tests.java`,
/// or starts with `Test`.
fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    stem.ends_with("Test") || stem.ends_with("Tests") || stem.starts_with("Test")
}

#[cfg(test)]
#[path = "java_tests.rs"]
mod tests;
