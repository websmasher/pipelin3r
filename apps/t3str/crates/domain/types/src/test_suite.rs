//! Aggregated test suite results.

use serde::{Deserialize, Serialize};

use crate::{Language, TestResult};

/// Summary counts for a test suite execution.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TestSummary {
    /// Total number of tests.
    pub total: u32,
    /// Number of passing tests.
    pub passed: u32,
    /// Number of failing tests.
    pub failed: u32,
    /// Number of skipped tests.
    pub skipped: u32,
    /// Number of errored tests.
    pub errors: u32,
}

/// Complete results from running a test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// Language of the test suite.
    pub language: Language,
    /// Repository directory that was tested.
    pub repo_dir: String,
    /// Individual test results.
    pub results: Vec<TestResult>,
    /// Aggregated summary.
    pub summary: TestSummary,
    /// Raw stdout/stderr from the test command, if captured.
    pub raw_output: Option<String>,
}
