//! C# test discovery via tree-sitter.

use std::path::Path;

use t3str_domain_types::Language;

use crate::DiscoverResult;
use crate::helpers;

/// Tree-sitter query for finding C# test methods.
///
/// Matches methods annotated with `[Test]` (`NUnit`), `[Fact]` (`xUnit`),
/// or `[TestMethod]` (`MSTest`) attributes.
const QUERY: &str = r#"
(method_declaration
  (attribute_list
    (attribute
      name: (identifier) @attr
      (#match? @attr "^(Test|Fact|TestMethod)$")))
  name: (identifier) @name)
"#;

/// Discover C# tests in the given directory.
///
/// Walks the directory for `.cs` files that follow common test file naming
/// conventions, then uses tree-sitter to extract methods annotated with
/// test framework attributes (`[Test]`, `[Fact]`, `[TestMethod]`).
pub fn discover(repo_dir: &Path, topic_filter: Option<&str>) -> DiscoverResult {
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    let files = helpers::find_test_files(repo_dir, &["cs"], &is_test_file)?;
    helpers::discover_in_files(&files, Language::Csharp, &lang, QUERY, topic_filter)
}

/// Check if a C# file is a test file.
///
/// Returns `true` if the filename ends with `Test.cs`, `Tests.cs`, or
/// the file stem starts with `Test`.
fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    stem.ends_with("Test") || stem.ends_with("Tests") || stem.starts_with("Test")
}

#[cfg(test)]
#[path = "csharp_tests.rs"]
mod tests;
