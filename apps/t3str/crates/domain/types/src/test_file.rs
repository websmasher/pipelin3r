//! Test file representation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Language;

/// A discovered test file with its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFile {
    /// Absolute path to the test file.
    pub path: PathBuf,
    /// Language of the test file.
    pub language: Language,
    /// Test function/method names discovered in this file.
    pub functions: Vec<String>,
    /// Whether this file is relevant to the requested topic filter.
    pub relevant: bool,
}
