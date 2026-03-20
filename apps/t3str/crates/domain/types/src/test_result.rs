//! Individual test result.

use serde::{Deserialize, Serialize};

/// Status of a single test execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    /// Test passed.
    Passed,
    /// Test failed.
    Failed,
    /// Test was skipped or ignored.
    Skipped,
    /// Test produced an error (distinct from assertion failure).
    Error,
}

/// Result of a single test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Fully qualified test name.
    pub name: String,
    /// Pass/fail/skip/error status.
    pub status: TestStatus,
    /// Duration in milliseconds, if available.
    pub duration_ms: Option<u64>,
    /// Failure or error message, if any.
    pub message: Option<String>,
    /// Source file path, if known.
    pub file: Option<String>,
}
