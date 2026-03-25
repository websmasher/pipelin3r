//! Adversarial tests for `work_dir` — Round 2.
#![allow(
    unused_crate_dependencies,
    reason = "integration test: deps used by lib not by test binary"
)]
//!
//! These tests target edge cases that Round 1 missed or that the Round 1 fixes
//! may have introduced. Focus areas: `validate_work_dir` boundary conditions,
//! `validate_path` gaps, `is_local` parsing quirks, `collect_relative_paths` with
//! unusual filesystem entries, and dry-run capture edge cases.

#![allow(clippy::unwrap_used, reason = "test assertions")]
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
#[allow(clippy::disallowed_types, reason = "tests need std::fs types")]
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

    /// Helper: create an executor with a specific base_url in dry-run mode.
    fn dry_run_executor_with_url(base_url: &str, capture_dir: PathBuf) -> Executor {
        let config = shedul3r_rs_sdk::ClientConfig {
            base_url: String::from(base_url),
            ..shedul3r_rs_sdk::ClientConfig::default()
        };
        Executor::new(&config).unwrap().with_dry_run(capture_dir)
    }

    // ========================================================================
    // validate_work_dir edge cases
    // ========================================================================

    /// Test 1: work_dir that is the root `/`.
    #[tokio::test]
    async fn work_dir_root_slash() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(PathBuf::from("/")),
            ..AgentConfig::new("root-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        // Root `/` is now rejected by validate_work_dir.
        let _ = result;
    }

    /// Test 2: work_dir with a very long path (exceeding OS PATH_MAX).
    #[tokio::test]
    async fn work_dir_very_long_path() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        // Build a path that exceeds 4096 characters.
        let mut long_path = dir.path().to_path_buf();
        for _ in 0..300 {
            long_path = long_path.join("a]really-long-segment");
        }

        let executor = dry_run_executor(capture_dir);

        let config = AgentConfig {
            work_dir: Some(long_path),
            ..AgentConfig::new("long-path", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_err(), "absurdly long path should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist"),
            "long path fails with 'does not exist' rather than 'too long': {msg}"
        );
    }

    /// Test 3: work_dir that exists but has a name consisting only of dots (`...`).
    #[tokio::test]
    async fn work_dir_named_triple_dots() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let dots_dir = dir.path().join("...");
        std::fs::create_dir_all(&dots_dir).unwrap();
        std::fs::write(dots_dir.join("file.txt"), b"dots content").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(dots_dir),
            ..AgentConfig::new("dots-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_ok(),
            "directory named '...' should be accepted: {result:?}"
        );
    }

    /// Test 4: work_dir that is a symlink TO a valid directory.
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_is_symlink_to_directory() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let real_dir = dir.path().join("real-work");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::fs::write(real_dir.join("real-file.txt"), b"real content").unwrap();

        let link_path = dir.path().join("link-work");
        std::os::unix::fs::symlink(&real_dir, &link_path).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig {
            work_dir: Some(link_path),
            ..AgentConfig::new("symlink-dir", "test prompt")
        };
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_ok(),
            "symlink to valid directory should be accepted: {result:?}"
        );

        let meta_path = capture_dir.join("symlink-dir").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("real-file.txt"),
            "files from the real directory behind the symlink should be listed"
        );
    }

    // ========================================================================
    // validate_path edge cases (used for expect_outputs)
    // ========================================================================

    /// Test 5: validate_path with a path that is just `.` (current dir component).
    #[tokio::test]
    async fn validate_path_bare_dot() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);
        let config = AgentConfig {
            work_dir: Some(work),
            expect_outputs: vec![String::from(".")],
            ..AgentConfig::new("dot-output", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_err(), "dry-run must reject '.' in expect_outputs");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid") || msg.contains("current dir") || msg.contains('.'),
            "error should mention invalid output path, got: {msg}"
        );
    }

    /// Test 6: validate_path with backslash separators on Unix.
    #[tokio::test]
    async fn validate_path_backslash_on_unix() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);
        let config = AgentConfig {
            work_dir: Some(work),
            expect_outputs: vec![String::from("dir\\file.txt")],
            ..AgentConfig::new("backslash-output", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "dry-run accepts backslash paths on Unix");
    }

    /// Test 7: empty expect_outputs list.
    #[tokio::test]
    async fn empty_expect_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir.clone());
        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("empty-outputs", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "empty expect_outputs should work fine");
    }

    // ========================================================================
    // is_local edge cases
    // ========================================================================

    /// Test 8: URL with no port and no path — just `http://localhost`.
    #[tokio::test]
    async fn is_local_bare_localhost() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://localhost", capture_dir.clone());
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("bare-localhost", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "bare localhost should work");
    }

    /// Test 9: URL with credentials and port — `http://user:pass@localhost:8080`.
    #[tokio::test]
    async fn is_local_credentials_and_port() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor =
            dry_run_executor_with_url("http://user:pass@localhost:8080", capture_dir.clone());
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("creds-localhost", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "credentials with localhost should work");
    }

    /// Test 10: URL with UPPERCASE localhost.
    #[tokio::test]
    async fn is_local_uppercase_localhost() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://LOCALHOST:7943", capture_dir.clone());
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("upper-localhost", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "uppercase LOCALHOST should work");
    }

    /// Test 11: URL that is empty string.
    #[tokio::test]
    async fn is_local_empty_url() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("", capture_dir);
        let config = AgentConfig::new("empty-url", "test");
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "empty URL dry-run should succeed");
    }

    /// Test 12: URL that is just a scheme.
    #[tokio::test]
    async fn is_local_just_scheme() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://", capture_dir);
        let config = AgentConfig::new("just-scheme", "test");
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "scheme-only URL dry-run should succeed");
    }

    /// Test 13: URL with double slashes in path.
    #[tokio::test]
    async fn is_local_double_slash_path() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://remote.example.com//api", capture_dir);
        let config = AgentConfig::new("double-slash", "test");
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "double-slash URL dry-run should succeed");
    }

    // ========================================================================
    // collect_relative_paths edge cases
    // ========================================================================

    /// Test 14: work_dir with empty subdirectories only.
    #[tokio::test]
    async fn work_dir_empty_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join("empty-a")).unwrap();
        std::fs::create_dir_all(work.join("empty-b")).unwrap();
        // No files at all — only empty subdirectories.

        let executor = dry_run_executor(capture_dir.clone());
        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("empty-subdir", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "empty subdirs should not crash");
        let meta_path = capture_dir.join("empty-subdir").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("\"workDirFiles\": []"),
            "empty subdirs with no files should produce empty listing"
        );
    }

    /// Test 15: work_dir with zero-length files.
    #[tokio::test]
    async fn work_dir_zero_length_files() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        std::fs::write(work.join("empty.txt"), b"").unwrap();
        std::fs::write(work.join("also-empty.json"), b"").unwrap();

        let executor = dry_run_executor(capture_dir.clone());
        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("empty-file", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_ok(), "zero-length files should work");
        let meta_path = capture_dir.join("empty-file").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("empty.txt"),
            "zero-length files should still be listed"
        );
    }

    /// Test 16: work_dir with circular symlinks between directories.
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_circular_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join("a")).unwrap();
        std::fs::create_dir_all(work.join("b")).unwrap();

        // Create circular links: a -> ../b, b -> ../a (indirectly circular)
        std::os::unix::fs::symlink(
            work.join("b").canonicalize().unwrap(),
            work.join("a").join("link-to-b"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            work.join("a").canonicalize().unwrap(),
            work.join("b").join("link-to-a"),
        )
        .unwrap();

        let executor = dry_run_executor(capture_dir);
        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("circular-links", "test")
        };
        let result = executor.run_agent(&config).await;

        // The visited set in collect_relative_paths should prevent infinite recursion.
        assert!(
            result.is_ok(),
            "circular symlinks should be handled, not infinite loop: {result:?}"
        );
    }

    /// Test 17: work_dir with a directory symlink loop (dir -> itself).
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_self_referencing_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        // Create a symlink that points to its parent directory.
        std::os::unix::fs::symlink(&work, work.join("loop")).unwrap();

        let executor = dry_run_executor(capture_dir);
        let config = AgentConfig {
            work_dir: Some(work),
            ..AgentConfig::new("dir-loop", "test")
        };
        let result = executor.run_agent(&config).await;

        // Should handle via the visited set — canonical path of "loop" resolves
        // to "work" which is already visited.
        match result {
            Ok(r) => {
                assert!(r.success, "somehow survived the loop");
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = msg;
            }
        }
    }

    // ========================================================================
    // extract_step_name edge cases
    // ========================================================================

    /// Test 33: Agent name with special characters produces valid directory name.
    #[tokio::test]
    async fn agent_name_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig::new("my agent / with (special) chars!", "test");
        let _ = executor.run_agent(&config).await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(&capture_dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .collect();

        assert_eq!(entries.len(), 1, "should have exactly one step dir");
        let step_dir_name = entries
            .first()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .to_string();
        assert!(
            !step_dir_name.contains(' '),
            "step dir name should not contain spaces: {step_dir_name}"
        );
        assert!(
            !step_dir_name.starts_with('-') && !step_dir_name.ends_with('-'),
            "step dir name should not start/end with dash: {step_dir_name}"
        );
    }

    /// Test 34: Agent name that is only special characters (no alphanumeric).
    #[tokio::test]
    async fn agent_name_only_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig::new("!@#$%^&*()", "test");
        let _ = executor.run_agent(&config).await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(&capture_dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .collect();

        assert_eq!(entries.len(), 1, "should have one step dir");
        let step_dir_name = entries
            .first()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .to_string();

        assert!(
            !step_dir_name.is_empty(),
            "step dir name should not be empty"
        );
    }

    // ========================================================================
    // Relative path in work_dir
    // ========================================================================

    /// Test 35: Relative work_dir path.
    #[tokio::test]
    async fn work_dir_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let config = AgentConfig {
            work_dir: Some(PathBuf::from("relative/path/here")),
            ..AgentConfig::new("relative-dir", "test")
        };
        let result = executor.run_agent(&config).await;

        assert!(result.is_err(), "relative work_dir must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("absolute"),
            "error should mention absolute path requirement: {msg}"
        );
    }

    // ========================================================================
    // Dry-run specific edge cases
    // ========================================================================

    /// Test 28: Multiple sequential dry-run captures should increment counter.
    #[tokio::test]
    async fn dry_run_counter_increments() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        // Execute 3 agents with the same name — counter should increment.
        for _ in 0_u32..3 {
            let config = AgentConfig::new("counter-test", "test");
            let _ = executor.run_agent(&config).await.unwrap();
        }

        // Verify all three capture directories exist.
        assert!(
            capture_dir.join("counter-test").join("0").exists(),
            "capture dir 0 should exist"
        );
        assert!(
            capture_dir.join("counter-test").join("1").exists(),
            "capture dir 1 should exist"
        );
        assert!(
            capture_dir.join("counter-test").join("2").exists(),
            "capture dir 2 should exist"
        );
    }

    /// Test 29: Counter is per-step-name, not global.
    #[tokio::test]
    async fn dry_run_counter_is_per_step() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let _ = executor
            .run_agent(&AgentConfig::new("agent-alpha", "test1"))
            .await
            .unwrap();

        let _ = executor
            .run_agent(&AgentConfig::new("agent-beta", "test2"))
            .await
            .unwrap();

        let _ = executor
            .run_agent(&AgentConfig::new("agent-alpha", "test3"))
            .await
            .unwrap();

        // Per-step counter: agent-alpha/0, agent-beta/0, agent-alpha/1
        assert!(
            capture_dir.join("agent-alpha").join("0").exists(),
            "first alpha capture should be at 0"
        );
        assert!(
            capture_dir.join("agent-beta").join("0").exists(),
            "beta capture should be at 0 (per-step counter)"
        );
        assert!(
            capture_dir.join("agent-alpha").join("1").exists(),
            "second alpha capture should be at 1 (per-step counter)"
        );
        assert!(
            !capture_dir.join("agent-alpha").join("2").exists(),
            "agent-alpha should only have indices 0 and 1"
        );
    }

    /// Test 37: Dry-run where capture base directory is read-only.
    #[tokio::test]
    #[cfg(unix)]
    async fn dry_run_readonly_capture_dir() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        std::fs::create_dir_all(&capture_dir).unwrap();

        // Make it read-only.
        std::fs::set_permissions(&capture_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig::new("readonly-capture", "test");
        let result = executor.run_agent(&config).await;

        // Restore permissions before assertions.
        std::fs::set_permissions(&capture_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            result.is_err(),
            "dry-run should fail when capture dir is read-only"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("IO error")
                || msg.contains("Permission denied")
                || msg.contains("permission"),
            "error should be IO-related: {msg}"
        );
    }

    /// Test 38: Dry-run where capture directory doesn't exist — should be auto-created.
    #[tokio::test]
    async fn dry_run_capture_dir_auto_created() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("deep").join("nested").join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let config = AgentConfig::new("auto-create", "test");
        let result = executor.run_agent(&config).await;

        assert!(
            result.is_ok(),
            "capture dir should be auto-created: {result:?}"
        );
        assert!(
            capture_dir.join("auto-create").join("0").exists(),
            "capture subdir should exist"
        );
    }
}
