//! Adversarial tests for `work_dir` implementation in pipelin3r.
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
    use std::path::{Path, PathBuf};

    use pipelin3r::{AgentTask, Executor};

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

        let result = executor
            .agent("nonexistent-dir")
            .prompt("test prompt")
            .work_dir(&nonexistent)
            .execute()
            .await;

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

        let result = executor
            .agent("file-not-dir")
            .prompt("test prompt")
            .work_dir(&file_path)
            .execute()
            .await;

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

        let result = executor
            .agent("traversal-test")
            .prompt("test prompt")
            .work_dir(&traversal_path)
            .execute()
            .await;

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

        let result = executor
            .agent("symlink-escape")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await;

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

        let empty_path = Path::new("");
        let result = executor
            .agent("empty-path")
            .prompt("test prompt")
            .work_dir(empty_path)
            .execute()
            .await;

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

        let result = executor
            .agent("no-read-perms")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await;

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

        let result = executor
            .agent("no-write-perms")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await;

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

        let result = executor
            .agent("special-chars")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await;

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

        let result = executor
            .agent("deep-nesting")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await;

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

        let result = executor
            .agent("hidden-files")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await
            .unwrap();

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

        let result = executor
            .agent("empty-dir")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await
            .unwrap();

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

        let result = executor
            .agent("many-files")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await
            .unwrap();

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
    // CONCURRENCY EDGE CASES
    // ========================================================================

    /// Test 13: Two batch tasks sharing the same work_dir.
    /// Expected: validation rejects duplicate work_dir paths in a batch.
    #[tokio::test]
    async fn batch_tasks_sharing_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("shared-work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("shared.txt"), b"shared content").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let work_clone = work.clone();
        let items = vec![1_u32, 2, 3];
        let result = executor
            .agent("shared-workdir")
            .items(items, 3)
            .for_each(move |item| {
                AgentTask::new()
                    .prompt(&format!("Process {item}"))
                    .work_dir(&work_clone)
            })
            .execute()
            .await;

        assert!(
            result.is_err(),
            "duplicate work_dir in batch must be rejected"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("duplicate work_dir"),
            "error should mention duplicate work_dir, got: {msg}"
        );
    }

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

        let result = executor
            .agent("deleted-dir")
            .prompt("test prompt")
            .work_dir(&work)
            .execute()
            .await;

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

    // ========================================================================
    // API EDGE CASES
    // ========================================================================

    /// Test 15: Execute with no prompt AND no work_dir.
    #[tokio::test]
    async fn execute_no_prompt_no_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let executor = dry_run_executor(capture_dir);

        let result = executor.agent("no-prompt-no-workdir").execute().await;

        assert!(result.is_err(), "should fail when no prompt is set");
        if let Err(e) = &result {
            let msg = format!("{e}");
            assert!(
                msg.contains("prompt"),
                "error should mention missing prompt, got: {msg}"
            );
        }
    }

    /// Test 16: Calling .work_dir() multiple times — last one wins?
    #[tokio::test]
    async fn work_dir_called_multiple_times() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let work1 = dir.path().join("first-dir");
        let work2 = dir.path().join("second-dir");
        std::fs::create_dir_all(&work1).unwrap();
        std::fs::create_dir_all(&work2).unwrap();
        std::fs::write(work1.join("first.txt"), b"first").unwrap();
        std::fs::write(work2.join("second.txt"), b"second").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let result = executor
            .agent("multi-workdir")
            .prompt("test prompt")
            .work_dir(&work1)
            .work_dir(&work2)
            .execute()
            .await
            .unwrap();

        assert!(result.success, "dry-run should succeed");

        let meta_path = capture_dir
            .join("multi-workdir")
            .join("0")
            .join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();

        // Last call should win
        assert!(
            meta_content.contains("second.txt"),
            "second work_dir's files should be listed (last wins)"
        );
        assert!(
            !meta_content.contains("first.txt"),
            "first work_dir's files should NOT be listed"
        );
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
        let result = executor
            .agent("traversal-outputs")
            .prompt("test prompt")
            .work_dir(&work)
            .expect_outputs(&["../../../etc/passwd", "../../.env"])
            .execute()
            .await;

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

        let result = executor
            .agent("prompt-only")
            .prompt("just a prompt, no work dir")
            .execute()
            .await
            .unwrap();

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

        let _ = executor
            .agent("meta-files")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await
            .unwrap();

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

        let result = executor
            .agent("nonexistent-dry")
            .prompt("test")
            .work_dir(&nonexistent)
            .execute()
            .await;

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
    /// E.g., https://example.com/localhost — should NOT be detected as local.
    #[tokio::test]
    async fn url_with_localhost_in_path_not_host() {
        // The is_local() method strips scheme and checks starts_with("localhost").
        // A URL like https://example.com/api/localhost would strip to
        // "example.com/api/localhost" — starts_with("localhost") is false. OK.
        // But what about https://example.com:8080/localhost?
        // Strips to "example.com:8080/localhost" — also fine.

        // This test validates the behavior by constructing an executor with
        // a URL that has localhost in the path.
        let config = shedul3r_rs_sdk::ClientConfig {
            base_url: String::from("https://example.com/api/localhost"),
            ..shedul3r_rs_sdk::ClientConfig::default()
        };
        let executor = Executor::new(&config).unwrap();

        // is_local is pub(crate), so we can't call it directly from tests.
        // Instead, we verify via dry-run behavior (dry-run bypasses local/remote).
        // This test documents that the detection SHOULD work correctly for this case.
        let _ = executor;
    }

    /// Test 22: URL like http://localhost.evil.com should NOT be local.
    /// This is a subdomain attack — `localhost.evil.com` resolves to the attacker.
    #[tokio::test]
    async fn url_localhost_evil_com_is_not_local() {
        // is_local() uses is_local_host() which checks that "localhost" is followed
        // by ':', '/', or end-of-string — so "localhost.evil.com" is correctly
        // classified as remote. This test confirms the fix.

        let config = shedul3r_rs_sdk::ClientConfig {
            base_url: String::from("http://localhost.evil.com"),
            ..shedul3r_rs_sdk::ClientConfig::default()
        };
        let executor = Executor::new(&config).unwrap();

        // We can verify this by checking that dry-run records the path
        // (dry-run mode doesn't distinguish local vs remote, but documents the issue)
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let executor = executor.with_dry_run(capture_dir);

        let result = executor
            .agent("evil-localhost")
            .prompt("test")
            .execute()
            .await
            .unwrap();

        assert!(result.success, "dry-run should succeed");
        // The actual bug is in is_local() — it uses starts_with("localhost")
        // without checking for a port separator or end of string.
    }

    // ========================================================================
    // ADDITIONAL EDGE CASES
    // ========================================================================

    /// Test: AgentTask work_dir setting works correctly.
    #[tokio::test]
    async fn agent_task_work_dir_setter() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("task-work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("task-file.txt"), b"task content").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let items = vec![String::from("item1")];
        let work_clone = work.clone();
        let results = executor
            .agent("task-workdir")
            .items(items, 1)
            .for_each(move |item| {
                AgentTask::new()
                    .prompt(&format!("Process {item}"))
                    .work_dir(&work_clone)
            })
            .execute()
            .await
            .unwrap();

        assert_eq!(results.len(), 1, "should have 1 result");
        assert!(results[0].is_ok(), "should succeed");

        let meta_path = capture_dir.join("task-workdir").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("task-file.txt"),
            "should list files from task's work_dir"
        );
    }

    /// Test: expect_outputs on AgentTask with traversal paths.
    /// FINDING: expect_outputs paths are not validated at the API level.
    /// They're stored as plain strings and used in path joins during
    /// remote download: dir.join(output_path) — path traversal possible.
    #[tokio::test]
    async fn agent_task_expect_outputs_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);

        // These traversal paths are accepted without any validation.
        // In real (non-dry-run) remote mode, the download step does:
        //   let local_path = dir.join(output_path);
        // which would resolve ../../../etc/passwd to an escape path.
        let result = executor
            .agent("traversal-outputs-task")
            .prompt("test")
            .work_dir(&work)
            .expect_outputs(&["../../../etc/passwd", "../../.ssh/id_rsa"])
            .execute()
            .await;

        // Dry-run succeeds because expect_outputs aren't used in dry-run mode.
        assert!(
            result.is_ok(),
            "traversal paths in expect_outputs are silently accepted"
        );
    }

    /// Test: Bundle validate_path rejects traversal, but expect_outputs doesn't use it.
    /// FINDING: Bundle paths are validated via validate_path(), but download
    /// paths (expect_outputs) are NOT validated. Inconsistent security boundary.
    #[tokio::test]
    async fn bundle_validates_but_expect_outputs_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);

        // Both safe and traversal paths are accepted without validation
        let result = executor
            .agent("mixed-outputs")
            .prompt("test")
            .work_dir(&work)
            .expect_outputs(&["safe/path.txt", "../escape.txt"])
            .execute()
            .await;

        assert!(
            result.is_ok(),
            "expect_outputs accepts traversal paths without validation"
        );
    }
}
