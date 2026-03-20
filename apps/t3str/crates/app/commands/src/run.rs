//! Run command -- orchestrates test execution.

use std::path::Path;

use t3str_discovery_port::TestExecutor;
use t3str_domain_types::{Language, T3strError, TestSuite};

/// Command for executing tests in a repository.
#[derive(Debug)]
pub struct RunCommand;

impl RunCommand {
    /// Run tests in the given repository.
    ///
    /// # Errors
    ///
    /// Returns `T3strError` if the underlying executor fails due to runner
    /// not found, runner failure, output parse errors, or I/O issues.
    pub async fn run(
        executor: &(impl TestExecutor + ?Sized),
        repo_dir: &Path,
        language: Language,
        filter: Option<&str>,
    ) -> Result<TestSuite, T3strError> {
        executor.execute(repo_dir, language, filter).await
    }
}
