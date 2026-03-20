//! Outbound port traits for test discovery and execution.

use std::path::Path;

use t3str_domain_types::{Language, T3strError, TestFile, TestSuite};

/// Alias for the result of test discovery — a list of discovered test files.
pub type DiscoveryResult = Result<Vec<TestFile>, T3strError>;

/// Port trait for discovering tests in a repository.
pub trait TestDiscoverer: Send + Sync {
    /// Discover test files and functions in the given directory.
    ///
    /// # Errors
    ///
    /// Returns `T3strError` if discovery fails due to I/O errors, parse
    /// failures, or language detection issues.
    fn discover(
        &self,
        repo_dir: &Path,
        language: Language,
        topic_filter: Option<&str>,
    ) -> DiscoveryResult;
}

/// Port trait for executing tests in a repository.
pub trait TestExecutor: Send + Sync {
    /// Execute tests and return structured results.
    ///
    /// # Errors
    ///
    /// Returns `T3strError` if execution fails due to runner not found,
    /// runner failure, output parse errors, or I/O issues.
    fn execute(
        &self,
        repo_dir: &Path,
        language: Language,
        filter: Option<&str>,
    ) -> impl std::future::Future<Output = Result<TestSuite, T3strError>> + Send;
}
