//! Extract command -- orchestrates test discovery.

use std::path::Path;

use t3str_discovery_port::{DiscoveryResult, TestDiscoverer};
use t3str_domain_types::Language;

/// Command for discovering and extracting test information from a repository.
#[derive(Debug)]
pub struct ExtractCommand;

impl ExtractCommand {
    /// Run test extraction on the given repository.
    ///
    /// # Errors
    ///
    /// Returns `T3strError` if the underlying discoverer fails due to I/O
    /// errors, parse failures, or language detection issues.
    pub fn run(
        discoverer: &impl TestDiscoverer,
        repo_dir: &Path,
        language: Language,
        topic_filter: Option<&str>,
    ) -> DiscoveryResult {
        discoverer.discover(repo_dir, language, topic_filter)
    }
}
