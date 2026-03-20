//! Extract adapter — tree-sitter based test discovery per language.

mod csharp;
mod fs;
mod elixir;
mod go;
mod helpers;
mod java;
mod javascript;
mod php;
mod python;
mod ruby;
mod rust_lang;

use std::path::Path;

use t3str_discovery_port::TestDiscoverer;
use t3str_domain_types::{Language, T3strError, TestFile};

/// Result type for per-language discovery functions.
pub(crate) type DiscoverResult = Result<Vec<TestFile>, T3strError>;

/// Tree-sitter based test discoverer supporting multiple languages.
///
/// Delegates to per-language modules that use tree-sitter grammars
/// to parse source files and identify test functions.
#[derive(Debug)]
pub struct TreeSitterDiscoverer;

impl TestDiscoverer for TreeSitterDiscoverer {
    fn discover(
        &self,
        repo_dir: &Path,
        language: Language,
        topic_filter: Option<&str>,
    ) -> Result<Vec<TestFile>, T3strError> {
        match language {
            Language::Rust => rust_lang::discover(repo_dir, topic_filter),
            Language::Python => python::discover(repo_dir, topic_filter),
            Language::Go => go::discover(repo_dir, topic_filter),
            Language::Javascript => javascript::discover(repo_dir, topic_filter),
            Language::Php => php::discover(repo_dir, topic_filter),
            Language::Csharp => csharp::discover(repo_dir, topic_filter),
            Language::Ruby => ruby::discover(repo_dir, topic_filter),
            Language::Java => java::discover(repo_dir, topic_filter),
            Language::Elixir => elixir::discover(repo_dir, topic_filter),
        }
    }
}
