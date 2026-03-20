//! Run adapter — test execution and output parsing per language.

mod csharp;
mod elixir;
mod go;
mod helpers;
mod java;
mod javascript;
pub mod parsers;
mod php;
mod python;
mod ruby;
mod rust_lang;

use std::path::Path;

use t3str_discovery_port::TestExecutor;
use t3str_domain_types::{Language, T3strError, TestSuite};

/// Process-based test executor supporting multiple languages.
#[derive(Debug)]
pub struct ProcessTestExecutor;

impl TestExecutor for ProcessTestExecutor {
    async fn execute(
        &self,
        repo_dir: &Path,
        language: Language,
        filter: Option<&str>,
    ) -> Result<TestSuite, T3strError> {
        match language {
            Language::Rust => rust_lang::execute(repo_dir, filter).await,
            Language::Python => python::execute(repo_dir, filter).await,
            Language::Go => go::execute(repo_dir, filter).await,
            Language::Javascript => javascript::execute(repo_dir, filter).await,
            Language::Php => php::execute(repo_dir, filter).await,
            Language::Csharp => csharp::execute(repo_dir, filter).await,
            Language::Ruby => ruby::execute(repo_dir, filter).await,
            Language::Java => java::execute(repo_dir, filter).await,
            Language::Elixir => elixir::execute(repo_dir, filter).await,
        }
    }
}
