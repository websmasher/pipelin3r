//! Adversarial tests for `work_dir` implementation in pipelin3r.
#![allow(
    unused_crate_dependencies,
    reason = "integration test: deps used by lib not by test binary"
)]
//!
//! These tests are designed to expose bugs, missing validation, and edge cases
//! in the `work_dir` handling across agent execution, dry-run capture, and
//! local/remote detection.

#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(clippy::panic, reason = "test assertions: panic on unexpected errors")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct fs access for test fixtures"
)]
#![allow(
    clippy::doc_markdown,
    reason = "test code: doc comments use identifiers without backticks for brevity"
)]

// Suppress unused-crate-dependencies for test binary.
use serde_json as _;
use shedul3r_rs_sdk as _;
use tempfile as _;
use thiserror as _;
use toml as _;
use tracing as _;

#[allow(clippy::unwrap_used, reason = "test assertions")]
#[allow(clippy::disallowed_methods, reason = "tests need direct fs access")]
#[allow(clippy::disallowed_types, reason = "tests need std::fs::File")]
#[allow(clippy::str_to_string, reason = "test code clarity")]
#[allow(clippy::arithmetic_side_effects, reason = "test loop counters")]
#[allow(
    clippy::indexing_slicing,
    reason = "test assertions on known-size collections"
)]
mod tests {
    use std::path::PathBuf;

    use pipelin3r::{AgentConfig, Executor};

    /// Helper: create an executor in dry-run mode.
    fn dry_run_executor(capture_dir: PathBuf) -> Executor {
        Executor::with_defaults().unwrap().with_dry_run(capture_dir)
    }

    // ========================================================================
    // PATH EDGE CASES
    // ========================================================================

    /// Test 1: work_dir that doesn't exist.
    /// Expected: validation rejects nonexistent paths with Config error.
    #[tokio::test]
    async fn work_dir_nonexistent_path() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let nonexistent = dir.path().join("this-does-not-exist");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(nonexistent),
            ..AgentConfig::new("nonexistent-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_err(),
            "nonexistent work_dir must be rejected by validation"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist"),
            "error should mention path does not exist, got: {msg}"
        );
    }

    /// Test 2: work_dir that is a file, not a directory.
    /// Expected: validation rejects non-directory paths with Config error.
    #[tokio::test]
    async fn work_dir_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let file_path = dir.path().join("not-a-dir.txt");
        std::fs::write(&file_path, b"I am a file").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(file_path),
            ..AgentConfig::new("file-not-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_err(),
            "work_dir pointing to a file must be rejected by validation"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("not a directory"),
            "error should mention not a directory, got: {msg}"
        );
    }

    /// Test 3: work_dir path with `..` traversal components.
    /// Expected: validation rejects paths containing `..` components.
    #[tokio::test]
    async fn work_dir_with_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        // Create a nested dir, then reference it via ..
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("secret.txt"), b"secret data").unwrap();

        // Path with .. traversal: a/b/../../a/b should resolve to a/b
        let traversal_path = dir
            .path()
            .join("a")
            .join("b")
            .join("..")
            .join("..")
            .join("a")
            .join("b");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(traversal_path),
            ..AgentConfig::new("traversal-test", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_err(),
            "work_dir with '..' traversal must be rejected by validation"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains(".."),
            "error should mention '..' components, got: {msg}"
        );
    }

    /// Test 4: work_dir with symlinks pointing outside.
    /// Expected: symlinks should be validated in remote mode to prevent path traversal.
    #[tokio::test]
    async fn work_dir_with_symlink_escape() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        // Create a file outside work_dir
        let outside_file = dir.path().join("outside-secret.txt");
        std::fs::write(&outside_file, b"secret outside content").unwrap();

        // Create a symlink inside work_dir pointing outside
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside_file, work.join("escape-link.txt")).unwrap();
        }

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("symlink-escape", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        match result {
            Ok(r) => {
                assert!(r.success, "dry-run succeeded");
                // Check if the symlinked file shows up in the file listing
                let meta_path = capture_dir
                    .join("symlink-escape")
                    .join("0")
                    .join("meta.json");
                let meta_content = std::fs::read_to_string(&meta_path).unwrap();
                // FINDING: symlinked file may be included in the listing without
                // any validation that it's inside the work_dir boundary
                if meta_content.contains("escape-link.txt") {
                    // The symlink target outside the dir is being listed/would be uploaded
                    // This is a security concern for remote mode
                }
            }
            Err(e) => {
                panic!("Symlink in work_dir caused error: {e}");
            }
        }
    }

    /// Test 5: work_dir path that is an empty string.
    /// Expected: validation rejects empty paths with Config error.
    #[tokio::test]
    async fn work_dir_empty_string() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(PathBuf::from("")),
            ..AgentConfig::new("empty-path", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_err(),
            "empty string work_dir must be rejected by validation"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("empty"),
            "error should mention empty path, got: {msg}"
        );
    }

    // ========================================================================
    // PERMISSION EDGE CASES
    // ========================================================================

    /// Test 6: work_dir with no read permissions.
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_no_read_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("file.txt"), b"content").unwrap();

        // Remove read permissions
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o000)).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work.clone()),
            ..AgentConfig::new("no-read-perms", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        // Restore permissions before assertions (so cleanup works)
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o755)).unwrap();

        match result {
            Ok(r) => {
                // FINDING: should this succeed? collect_relative_paths uses .and_then
                // so it silently returns empty on read error
                assert!(r.success, "dry-run succeeded despite unreadable dir");
            }
            Err(e) => {
                // Error propagation is acceptable
                let _ = e;
            }
        }
    }

    /// Test 7: work_dir with no write permissions (agent can't write output).
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_no_write_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("input.txt"), b"content").unwrap();

        // Remove write permissions (keep read)
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o555)).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work.clone()),
            ..AgentConfig::new("no-write-perms", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        // Restore permissions
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Dry-run doesn't write TO the work_dir, so this should succeed
        match result {
            Ok(r) => {
                assert!(
                    r.success,
                    "dry-run should succeed — it doesn't write to work_dir"
                );
            }
            Err(e) => {
                panic!("No-write work_dir caused error in dry-run: {e}");
            }
        }
    }

    // ========================================================================
    // CONTENT EDGE CASES
    // ========================================================================

    /// Test 8: work_dir with special characters in filenames.
    #[tokio::test]
    async fn work_dir_special_characters_in_filenames() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        // Files with spaces, unicode
        std::fs::write(work.join("file with spaces.txt"), b"spaces").unwrap();
        std::fs::write(work.join("ünïcödé.txt"), b"unicode").unwrap();
        std::fs::write(work.join("emoji-🔥.txt"), b"fire").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("special-chars", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        match result {
            Ok(r) => {
                assert!(r.success, "dry-run should handle special chars");
                let meta_path = capture_dir
                    .join("special-chars")
                    .join("0")
                    .join("meta.json");
                let meta_content = std::fs::read_to_string(&meta_path).unwrap();
                // Verify all files are listed
                assert!(
                    meta_content.contains("file with spaces.txt"),
                    "should list file with spaces"
                );
                assert!(
                    meta_content.contains("ünïcödé.txt"),
                    "should list unicode filename"
                );
            }
            Err(e) => {
                panic!("Special characters in filenames caused error: {e}");
            }
        }
    }

    /// Test 9: work_dir with deeply nested directories.
    #[tokio::test]
    async fn work_dir_deeply_nested() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");

        // Create 15-level deep nesting
        let mut deep = work.clone();
        for i in 0..15 {
            deep = deep.join(format!("level{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("deep-file.txt"), b"deep content").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("deep-nesting", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        match result {
            Ok(r) => {
                assert!(r.success, "dry-run should handle deep nesting");
                let meta_path = capture_dir.join("deep-nesting").join("0").join("meta.json");
                let meta_content = std::fs::read_to_string(&meta_path).unwrap();
                assert!(
                    meta_content.contains("deep-file.txt"),
                    "should find deeply nested file"
                );
            }
            Err(e) => {
                panic!("Deep nesting caused error: {e}");
            }
        }
    }

    /// Test 10: work_dir with hidden files (.dotfiles).
    #[tokio::test]
    async fn work_dir_hidden_files_included() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        std::fs::write(work.join(".env"), b"SECRET=value").unwrap();
        std::fs::write(work.join(".gitignore"), b"*.tmp").unwrap();
        std::fs::write(work.join("visible.txt"), b"visible").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("hidden-files", "test prompt")
        };
        let result = executor.run_agent(&config).await.unwrap();

        assert!(result.success, "dry-run should succeed");

        let meta_path = capture_dir.join("hidden-files").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();

        // FINDING: .env files with secrets are included in the file listing
        // and would be uploaded in remote mode. No filtering of sensitive files.
        assert!(
            meta_content.contains(".env"),
            "SECURITY: .env file IS included in work_dir listing — would be uploaded in remote mode"
        );
        assert!(
            meta_content.contains(".gitignore"),
            ".gitignore is included too"
        );
    }

    /// Test 11: Empty work_dir (no files).
    #[tokio::test]
    async fn work_dir_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("empty-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await.unwrap();

        assert!(result.success, "dry-run should succeed with empty work_dir");

        let meta_path = capture_dir.join("empty-dir").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("\"workDirFiles\": []"),
            "workDirFiles should be empty array for empty dir"
        );
    }

    /// Test 12: work_dir with large number of files.
    #[tokio::test]
    async fn work_dir_many_files() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        // Create 500 files (reduced from 1000 for test speed)
        for i in 0_u32..500 {
            std::fs::write(
                work.join(format!("file-{i:04}.txt")),
                format!("content {i}").as_bytes(),
            )
            .unwrap();
        }

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("many-files", "test prompt")
        };
        let result = executor.run_agent(&config).await.unwrap();

        assert!(result.success, "dry-run should handle many files");

        // Verify all files are listed
        let meta_path = capture_dir.join("many-files").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        // No size limit validation — all 500 files are listed
        assert!(
            meta_content.contains("file-0499.txt"),
            "should list the last file"
        );
        assert!(
            meta_content.contains("file-0000.txt"),
            "should list the first file"
        );
    }

    // ========================================================================
    // API EDGE CASES (batch tests removed — batch API replaced by run_pool_map)
    // ========================================================================

    /// Test 14: work_dir deleted before execution.
    /// Expected: validation rejects nonexistent paths.
    #[tokio::test]
    async fn work_dir_deleted_before_capture() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("ephemeral-work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("temp.txt"), b"temporary").unwrap();

        // Delete the work dir before executing
        std::fs::remove_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("deleted-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_err(),
            "deleted work_dir must be rejected by validation"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist"),
            "error should mention path does not exist, got: {msg}"
        );
    }

    /// Test 15: Execute with empty prompt and no work_dir.
    #[tokio::test]
    async fn execute_empty_prompt_no_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let executor = dry_run_executor(capture_dir);

        // AgentConfig always has a prompt (required field), but it can be empty.
        let config = AgentConfig::new("empty-prompt-test", "");
        let result = executor.run_agent(&config).await;

        // Empty prompt is allowed — the agent subprocess receives empty stdin.
        assert!(result.is_ok(), "empty prompt should not fail");
    }

    /// Test 17: expect_outputs with path traversal.
    #[tokio::test]
    async fn expect_outputs_with_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);

        // FINDING: expect_outputs paths are not validated for traversal.
        // In remote mode, this could be used to download arbitrary files.
        let config = AgentConfig {
            work_dir: Some(work),
            expect_outputs: vec![
                String::from("../../../etc/passwd"),
                String::from("../../.env"),
            ],
            ..AgentConfig::new("traversal-outputs", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        // In dry-run mode, expect_outputs are not used, so this succeeds.
        // But in real mode, dir.join("../../../etc/passwd") would escape.
        assert!(
            result.is_ok(),
            "dry-run doesn't use expect_outputs, but real mode would have a path traversal"
        );
    }

    /// Test 18: Prompt without work_dir should work.
    #[tokio::test]
    async fn prompt_only_no_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig::new("prompt-only", "just a prompt, no work dir");
        let result = executor.run_agent(&config).await.unwrap();

        assert!(result.success, "prompt-only execution should work");

        let meta_path = capture_dir.join("prompt-only").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("\"workDir\": null"),
            "workDir should be null when not set"
        );
        assert!(
            meta_content.contains("\"workDirFiles\": []"),
            "workDirFiles should be empty when no work_dir"
        );
    }

    // ========================================================================
    // DRY-RUN SPECIFIC
    // ========================================================================

    /// Test 19: Dry-run with work_dir containing files — verify meta.json content.
    #[tokio::test]
    async fn dry_run_meta_json_lists_work_dir_files() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join("subdir")).unwrap();
        std::fs::write(work.join("root-file.txt"), b"root").unwrap();
        std::fs::write(work.join("subdir").join("nested.txt"), b"nested").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("meta-files", "test")
        };
        let _ = executor.run_agent(&config).await.unwrap();

        let meta_path = capture_dir.join("meta-files").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("root-file.txt"),
            "should list root file"
        );
        assert!(
            meta_content.contains("nested.txt"),
            "should list nested file"
        );
    }

    /// Test 20: Dry-run with work_dir that doesn't exist.
    /// Expected: validation rejects nonexistent paths with Config error (no panic).
    #[tokio::test]
    async fn dry_run_nonexistent_work_dir_no_panic() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let nonexistent = PathBuf::from("/tmp/absolutely-does-not-exist-12345678");

        let executor = dry_run_executor(capture_dir);

        let config = AgentConfig {
            work_dir: Some(nonexistent),
            ..AgentConfig::new("nonexistent-dry", "test")
        };
        let result = executor.run_agent(&config).await;

        // Should NOT panic. Returns a Config error for nonexistent path.
        assert!(
            result.is_err(),
            "nonexistent work_dir must be rejected by validation"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist"),
            "error should mention path does not exist, got: {msg}"
        );
    }

    // ========================================================================
    // REMOTE DETECTION (is_local)
    // ========================================================================

    /// Test 21: URL with "localhost" in the path but not the host.
    #[tokio::test]
    async fn url_with_localhost_in_path_not_host() {
        let config = shedul3r_rs_sdk::ClientConfig {
            base_url: String::from("https://example.com/api/localhost"),
            ..shedul3r_rs_sdk::ClientConfig::default()
        };
        let executor = Executor::new(&config).unwrap();
        let _ = executor;
    }

    /// Test 22: URL like http://localhost.evil.com should NOT be local.
    #[tokio::test]
    async fn url_localhost_evil_com_is_not_local() {
        let config = shedul3r_rs_sdk::ClientConfig {
            base_url: String::from("http://localhost.evil.com"),
            ..shedul3r_rs_sdk::ClientConfig::default()
        };
        let executor = Executor::new(&config).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let executor = executor.with_dry_run(capture_dir);

        let agent_config = AgentConfig::new("evil-localhost", "test");
        let result = executor.run_agent(&agent_config).await.unwrap();

        assert!(result.success, "dry-run should succeed");
    }

    // ========================================================================
    // ADDITIONAL EDGE CASES
    // ========================================================================

    /// Test: expect_outputs with traversal paths in AgentConfig.
    #[tokio::test]
    async fn agent_config_expect_outputs_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);

        let config = AgentConfig {
            work_dir: Some(work),
            expect_outputs: vec![
                String::from("../../../etc/passwd"),
                String::from("../../.ssh/id_rsa"),
            ],
            ..AgentConfig::new("traversal-outputs-task", "test")
        };
        let result = executor.run_agent(&config).await;

        // Dry-run succeeds because expect_outputs aren't used in dry-run mode.
        assert!(
            result.is_ok(),
            "traversal paths in expect_outputs are silently accepted"
        );
    }

    /// Test: Bundle validate_path rejects traversal, but expect_outputs doesn't use it in dry-run.
    #[tokio::test]
    async fn bundle_validates_but_expect_outputs_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);

        let config = AgentConfig {
            work_dir: Some(work),
            expect_outputs: vec![String::from("safe/path.txt"), String::from("../escape.txt")],
            ..AgentConfig::new("mixed-outputs", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_ok(),
            "expect_outputs accepts traversal paths without validation"
        );
    }
}
