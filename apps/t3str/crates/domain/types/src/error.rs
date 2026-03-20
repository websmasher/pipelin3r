//! Error types for t3str.

use crate::Language;

/// Errors that can occur during test extraction or execution.
#[derive(Debug, thiserror::Error)]
pub enum T3strError {
    /// The specified language is not supported.
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    /// The repository directory does not exist or is not accessible.
    #[error("repository not found: {path}")]
    RepoNotFound {
        /// Path that was not found.
        path: String,
    },

    /// No test framework was detected for the given language.
    #[error("no test framework detected for {language} in {repo_dir}")]
    NoTestFramework {
        /// Language that was being tested.
        language: Language,
        /// Repository directory.
        repo_dir: String,
    },

    /// Test command execution failed.
    #[error("test execution failed for {language}: {reason}")]
    ExecutionFailed {
        /// Language that was being tested.
        language: Language,
        /// Reason for the failure.
        reason: String,
    },

    /// Failed to parse test output.
    #[error("failed to parse {format} output: {reason}")]
    ParseFailed {
        /// Output format that failed to parse.
        format: String,
        /// Reason for the parse failure.
        reason: String,
    },

    /// IO error during file operations.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
