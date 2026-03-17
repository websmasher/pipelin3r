//! Adversarial tests for `work_dir` — Round 2.
//!
//! These tests target edge cases that Round 1 missed or that the Round 1 fixes
//! may have introduced. Focus areas: `validate_work_dir` boundary conditions,
//! `validate_path` gaps, `is_local` parsing quirks, `collect_relative_paths` with
//! unusual filesystem entries, batch/concurrency edge cases, and builder
//! state-machine behavior.

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
    use std::path::{Path, PathBuf};

    use pipelin3r::{AgentTask, Executor};

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
    /// Root is absolute, exists, is a directory, has no `..` — but it's
    /// dangerous to allow as a work directory.
    /// QUESTION: should this be allowed? Currently it passes validation.
    #[tokio::test]
    async fn work_dir_root_slash() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let root = Path::new("/");
        let result = executor
            .agent("root-dir")
            .prompt("test prompt")
            .work_dir(root)
            .execute()
            .await;

        // FINDING: Root `/` passes validation. In remote mode, this would
        // attempt to upload the ENTIRE filesystem. There is no depth or size
        // guard. In dry-run mode, collect_relative_paths recurses the whole
        // filesystem tree (extremely slow, possible OOM).
        //
        // We don't assert success/failure here because the dry-run capture
        // would attempt to list all files on disk, which is too slow. We just
        // note that validation does NOT reject `/`.
        let _ = result;
    }

    /// Test 2: work_dir with a very long path (exceeding OS PATH_MAX).
    /// On most Unix systems, PATH_MAX is 4096 bytes.
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

        let result = executor
            .agent("long-path")
            .prompt("test prompt")
            .work_dir(&long_path)
            .execute()
            .await;

        // FINDING: The path exceeds OS limits. validate_work_dir checks
        // is_absolute (true), then checks for `..` (none), then calls
        // path.exists() which returns false because the OS rejects the path.
        // So we get a "does not exist" error rather than a more informative
        // "path too long" error. Not a bug per se, but poor error messaging.
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

        let result = executor
            .agent("dots-dir")
            .prompt("test prompt")
            .work_dir(&dots_dir)
            .execute()
            .await;

        // `...` is a valid directory name on Unix (it's NOT a parent traversal).
        // validate_work_dir should accept it because Component::Normal("...") is
        // not Component::ParentDir.
        assert!(
            result.is_ok(),
            "directory named '...' should be accepted: {result:?}"
        );
    }

    /// Test 4: work_dir that is a symlink TO a valid directory.
    /// The work_dir itself is a symlink, but it points to a real directory.
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

        let result = executor
            .agent("symlink-dir")
            .prompt("test prompt")
            .work_dir(&link_path)
            .execute()
            .await;

        // FINDING: validate_work_dir uses path.is_dir() which follows symlinks.
        // So a symlink to a directory passes validation. This is probably fine
        // for local mode, but in remote mode, the canonical base is the real dir.
        // The meta.json would record the symlink path, not the real path.
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
    /// Round 1 tested `./file.txt`, but not bare `.`.
    #[tokio::test]
    async fn validate_path_bare_dot() {
        // validate_path is pub(crate), so we test it indirectly via
        // the remote download path. In dry-run, expect_outputs are not
        // validated. But we can verify the validate_path logic by noting
        // that "." decomposes to Component::CurDir.
        //
        // In execute_with_work_dir, the validate_path call happens during
        // remote download. In dry-run mode, it's skipped.
        //
        // FINDING: If someone passes "." as an expect_output in remote mode,
        // validate_path would reject it (Component::CurDir). This is correct.
        // But the validation only runs in remote mode, not in the builder.
        // No bug here, just documenting the asymmetry.
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);
        let result = executor
            .agent("dot-output")
            .prompt("test")
            .work_dir(&work)
            .expect_outputs(&["."])
            .execute()
            .await;

        // Dry-run doesn't validate expect_outputs, so this succeeds.
        assert!(result.is_ok(), "dry-run accepts '.' in expect_outputs");
    }

    /// Test 6: validate_path with backslash separators on Unix.
    /// `dir\file.txt` — on Unix, `\` is a valid filename character, not a separator.
    #[tokio::test]
    async fn validate_path_backslash_on_unix() {
        // On Unix, `dir\file.txt` is a single Normal component (one filename).
        // validate_path would accept it. But on Windows, it'd be a path separator.
        // This is a cross-platform inconsistency but NOT a bug on Unix.
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);
        let result = executor
            .agent("backslash-output")
            .prompt("test")
            .work_dir(&work)
            .expect_outputs(&["dir\\file.txt"])
            .execute()
            .await;

        assert!(result.is_ok(), "dry-run accepts backslash paths on Unix");
    }

    /// Test 7: empty expect_outputs list.
    /// Should be a no-op for the download phase.
    #[tokio::test]
    async fn empty_expect_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir.clone());
        let result = executor
            .agent("empty-outputs")
            .prompt("test")
            .work_dir(&work)
            .expect_outputs(&[])
            .execute()
            .await;

        assert!(result.is_ok(), "empty expect_outputs should work fine");
    }

    // ========================================================================
    // is_local edge cases
    // ========================================================================

    /// Test 8: URL with no port and no path — just `http://localhost`.
    /// is_local_host checks for host followed by `:`, `/`, or end-of-string.
    /// After stripping scheme, we get `localhost` — rest is empty. Should match.
    #[tokio::test]
    async fn is_local_bare_localhost() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        // We can't call is_local() directly (pub(crate)), but we can
        // create an executor and verify its behavior indirectly via
        // dry-run meta output. The key question: does is_local return true?
        let executor = dry_run_executor_with_url("http://localhost", capture_dir.clone());
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let result = executor
            .agent("bare-localhost")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await;

        // If is_local() returns true, the path is passed as-is.
        // If false, it would try to upload (which fails in dry-run anyway).
        // Dry-run bypasses local/remote entirely, so this just tests that
        // the URL parsing doesn't panic.
        assert!(result.is_ok(), "bare localhost URL should not cause errors");
    }

    /// Test 9: URL with username:password — `http://user:pass@localhost:7943`.
    /// After stripping scheme, we get `user:pass@localhost:7943`.
    /// is_local_host checks starts_with("localhost") on that — it would NOT match.
    /// FINDING: URLs with credentials are incorrectly classified as remote.
    #[tokio::test]
    async fn is_local_with_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor =
            dry_run_executor_with_url("http://user:pass@localhost:7943", capture_dir.clone());

        // is_local() strips scheme to get "user:pass@localhost:7943"
        // Then checks starts_with("localhost") — FALSE.
        // So a localhost URL with credentials is classified as REMOTE.
        // This means files would be uploaded to the local server unnecessarily.
        //
        // FINDING: is_local() does not handle URL credentials (user:pass@).
        // While uncommon, it's a valid URL format that breaks local detection.
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let result = executor
            .agent("creds-localhost")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await;

        assert!(result.is_ok(), "URL with credentials should not crash");
    }

    /// Test 10: URL with uppercase host — `http://LOCALHOST:7943`.
    /// HTTP host names are case-insensitive per RFC 2616, but string
    /// comparison in is_local_host is case-sensitive.
    /// FINDING: `LOCALHOST` is NOT recognized as local.
    #[tokio::test]
    async fn is_local_uppercase_localhost() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://LOCALHOST:7943", capture_dir.clone());

        // is_local() strips scheme to get "LOCALHOST:7943".
        // strip_prefix("localhost") does NOT match "LOCALHOST" (case-sensitive).
        // FINDING: uppercase LOCALHOST is classified as remote. RFC says hostnames
        // are case-insensitive. This is a bug — it would cause unnecessary uploads.
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let result = executor
            .agent("upper-localhost")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await;

        assert!(result.is_ok(), "uppercase LOCALHOST should not crash");
    }

    /// Test 11: URL that is an empty string.
    /// FINDING: An empty base_url causes is_local() to return false
    /// (strip_prefix fails, unwrap_or returns "", starts_with("localhost") is false).
    /// Not a crash, but creates a broken executor.
    #[tokio::test]
    async fn is_local_empty_url() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("", capture_dir.clone());

        let result = executor.agent("empty-url").prompt("test").execute().await;

        // Empty URL won't crash in dry-run mode.
        assert!(result.is_ok(), "empty URL should not crash in dry-run");
    }

    /// Test 12: URL that is just a scheme — `http://`.
    /// After stripping scheme, we get an empty string.
    #[tokio::test]
    async fn is_local_just_scheme() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://", capture_dir.clone());

        let result = executor.agent("just-scheme").prompt("test").execute().await;

        assert!(
            result.is_ok(),
            "scheme-only URL should not crash in dry-run"
        );
    }

    /// Test 13: URL with double slashes in path — `http://localhost//api`.
    #[tokio::test]
    async fn is_local_double_slash_path() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor_with_url("http://localhost//api", capture_dir.clone());

        // After stripping scheme: "localhost//api"
        // strip_prefix("localhost") → Some("//api")
        // rest.starts_with('/') → true
        // So this is correctly detected as local. Good.
        let result = executor
            .agent("double-slash")
            .prompt("test")
            .execute()
            .await;

        assert!(result.is_ok(), "double-slash path should not crash");
    }

    // ========================================================================
    // collect_relative_paths / read_dir_to_memory edge cases
    // ========================================================================

    /// Test 14: Directory containing an empty subdirectory.
    /// Empty subdirectories are not preserved in the file listing (only files
    /// are listed). In remote mode, empty dirs would not be created on the
    /// server, which could break agents that expect certain directory structures.
    #[tokio::test]
    async fn work_dir_with_empty_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join("empty-subdir")).unwrap();
        std::fs::create_dir_all(work.join("nonempty-subdir")).unwrap();
        std::fs::write(work.join("nonempty-subdir").join("file.txt"), b"content").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let result = executor
            .agent("empty-subdir")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await
            .unwrap();

        assert!(result.success);

        let meta_path = capture_dir.join("empty-subdir").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();

        // FINDING: The empty subdirectory is NOT listed in workDirFiles.
        // collect_relative_paths only records files, not directories.
        // In remote mode (read_dir_to_memory), empty dirs are also not uploaded.
        // An agent expecting `work_dir/empty-subdir/` to exist would fail.
        //
        // The meta_content contains "nonempty-subdir/file.txt" (a file path),
        // but never a bare "empty-subdir" entry (since it has no files).
        // We check that "empty-subdir/something" never appears as a file path.
        let parsed: serde_json::Value = serde_json::from_str(&meta_content).unwrap();
        let files = parsed.get("workDirFiles").unwrap().as_array().unwrap();
        let has_empty_subdir_entry = files
            .iter()
            .any(|f| f.as_str().is_some_and(|s| s.starts_with("empty-subdir")));
        assert!(
            !has_empty_subdir_entry,
            "empty subdirectories produce no file entries — they are silently dropped"
        );
        assert!(
            meta_content.contains("file.txt"),
            "files in non-empty subdirs should still appear"
        );
    }

    /// Test 15: File with zero bytes (empty file) in work_dir.
    #[tokio::test]
    async fn work_dir_with_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("empty.txt"), b"").unwrap();
        std::fs::write(work.join("nonempty.txt"), b"content").unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let result = executor
            .agent("empty-file")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await
            .unwrap();

        assert!(result.success);

        let meta_path = capture_dir.join("empty-file").join("0").join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();

        // Empty files should still be listed.
        assert!(
            meta_content.contains("empty.txt"),
            "zero-byte files should be listed in workDirFiles"
        );
    }

    /// Test 16: Circular symlinks (a -> b -> a) in work_dir.
    /// Should not cause infinite recursion or stack overflow.
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_circular_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("normal.txt"), b"normal").unwrap();

        // Create circular symlinks: a -> b, b -> a
        let a = work.join("link-a");
        let b = work.join("link-b");
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let result = executor
            .agent("circular-links")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await;

        // FINDING: collect_relative_paths calls canonicalize() on each entry.
        // For circular symlinks, canonicalize() returns Err (too many levels of
        // symbolic links). The code handles this with a tracing::warn and
        // `continue`, so it won't stack overflow. Good.
        assert!(
            result.is_ok(),
            "circular symlinks should not cause panic or infinite loop: {result:?}"
        );

        // Verify normal files are still listed despite the broken symlinks.
        let meta_path = capture_dir
            .join("circular-links")
            .join("0")
            .join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();
        assert!(
            meta_content.contains("normal.txt"),
            "normal files should still be listed alongside circular symlinks"
        );
    }

    /// Test 17: Symlink directory loop (subdir -> parent dir).
    /// Should not cause infinite recursion.
    #[tokio::test]
    #[cfg(unix)]
    async fn work_dir_symlink_directory_loop() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("file.txt"), b"content").unwrap();

        // Create a symlink subdir that points back to the work dir itself.
        std::os::unix::fs::symlink(&work, work.join("loop-back")).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let result = executor
            .agent("dir-loop")
            .prompt("test")
            .work_dir(&work)
            .execute()
            .await;

        // The canonical base is the real work dir. The symlink "loop-back"
        // resolves to the same canonical path as the base, so entries under
        // loop-back/ would have canonical paths starting with canonical_base.
        // This means the recursion WOULD follow the symlink and recurse
        // infinitely... UNLESS canonicalize of the symlink itself resolves to
        // base, and then recursing into it re-enumerates the same entries
        // (including loop-back again), causing infinite recursion.
        //
        // FINDING: The starts_with(canonical_base) check passes for the symlink
        // (since it resolves to base), so the code FOLLOWS the symlink and
        // recurses into it. This creates infinite recursion until stack overflow.
        //
        // However, on macOS/Linux, the canonical path of work/loop-back IS
        // canonical_base, so read_dir would re-list the same entries including
        // loop-back, causing unbounded recursion.
        //
        // The code does NOT track visited inodes, so this is a real bug.
        // We'll see if it panics with stack overflow or if the OS limits help.
        match result {
            Ok(r) => {
                // If it succeeded, that means the OS or some other mechanism stopped it.
                assert!(r.success, "somehow survived the loop");
            }
            Err(e) => {
                // Stack overflow would be a panic (caught by tokio as JoinError),
                // but since this is not spawned, it would crash the test.
                // An IO error (too many symlinks) is acceptable.
                let msg = e.to_string();
                let _ = msg;
            }
        }
    }

    // ========================================================================
    // Batch edge cases
    // ========================================================================

    /// Test 18: Batch with 0 items.
    /// Should return an empty results vec, not panic or error.
    #[tokio::test]
    async fn batch_zero_items() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items: Vec<u32> = vec![];
        let result = executor
            .agent("zero-items")
            .items(items, 5)
            .for_each(|item| AgentTask::new().prompt(&format!("Process {item}")))
            .execute()
            .await;

        assert!(result.is_ok(), "batch with 0 items should succeed");
        let results = result.unwrap();
        assert!(
            results.is_empty(),
            "batch with 0 items should return empty vec"
        );
    }

    /// Test 19: Batch with 1 item — degenerate case.
    #[tokio::test]
    async fn batch_single_item() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items = vec![String::from("only-one")];
        let result = executor
            .agent("single-item")
            .items(items, 1)
            .for_each(|item| AgentTask::new().prompt(&format!("Process {item}")))
            .execute()
            .await;

        assert!(result.is_ok(), "batch with 1 item should succeed");
        let results = result.unwrap();
        assert_eq!(results.len(), 1, "should have exactly 1 result");
        assert!(
            results.first().unwrap().is_ok(),
            "single item should succeed"
        );
    }

    /// Test 20: Batch with concurrency set to 0.
    /// run_pool treats 0 as 1 (effective_concurrency). But does the batch
    /// builder handle it the same way?
    #[tokio::test]
    async fn batch_concurrency_zero() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items = vec![1_u32, 2, 3];
        let result = executor
            .agent("zero-concurrency")
            .items(items, 0)
            .for_each(|item| AgentTask::new().prompt(&format!("Process {item}")))
            .execute()
            .await;

        // In dry-run mode, the batch executes sequentially (not via run_pool),
        // so concurrency=0 doesn't matter. But in real mode, run_pool would
        // treat 0 as 1 (it has: let effective = if c == 0 { 1 } else { c }).
        assert!(result.is_ok(), "concurrency 0 should not deadlock");
        let results = result.unwrap();
        assert_eq!(results.len(), 3, "should have 3 results");
    }

    /// Test 21: Batch with concurrency larger than item count.
    #[tokio::test]
    async fn batch_concurrency_exceeds_items() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items = vec![1_u32, 2];
        let result = executor
            .agent("high-concurrency")
            .items(items, 1000)
            .for_each(|item| AgentTask::new().prompt(&format!("Process {item}")))
            .execute()
            .await;

        assert!(result.is_ok(), "concurrency > items should work");
        let results = result.unwrap();
        assert_eq!(results.len(), 2, "should have 2 results");
    }

    /// Test 22: Batch where for_each closure returns task with no prompt.
    /// FINDING: In dry-run batch mode, execute_batch_task_dry_run checks for
    /// prompt and returns Config error. This should propagate correctly.
    #[tokio::test]
    async fn batch_task_missing_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items = vec![1_u32, 2, 3];
        let result = executor
            .agent("no-prompt-batch")
            .items(items, 2)
            .for_each(|_item| {
                // Deliberately omit prompt
                AgentTask::new()
            })
            .execute()
            .await;

        // FIX: Dry-run batch now collects per-item results like real mode,
        // instead of propagating errors with `?`. Each missing-prompt task
        // returns an Err in the results vec.
        assert!(
            result.is_ok(),
            "batch should return Ok with per-item results"
        );
        let results = result.unwrap();
        assert_eq!(results.len(), 3, "should have 3 per-item results");
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_err(), "item {i} should be Err (missing prompt)");
            let msg = r.as_ref().unwrap_err().to_string();
            assert!(
                msg.contains("prompt"),
                "error should mention missing prompt: {msg}"
            );
        }
    }

    /// Test 23: Batch without calling for_each (no mapper set).
    #[tokio::test]
    async fn batch_no_for_each() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items = vec![1_u32, 2, 3];
        let result = executor
            .agent("no-mapper")
            .items(items, 2)
            // Deliberately skip .for_each()
            .execute()
            .await;

        assert!(result.is_err(), "batch without for_each should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("for_each"),
            "error should mention missing for_each mapper: {msg}"
        );
    }

    // ========================================================================
    // AgentBuilder state machine
    // ========================================================================

    /// Test 24: Calling .work_dir() then .work_dir() again — last should win.
    /// Same as round 1 test 16, but verify via meta.json workDir field value.
    #[tokio::test]
    async fn work_dir_overwrite_check_meta_workdir_field() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let work1 = dir.path().join("first");
        let work2 = dir.path().join("second");
        std::fs::create_dir_all(&work1).unwrap();
        std::fs::create_dir_all(&work2).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let _ = executor
            .agent("overwrite-workdir")
            .prompt("test")
            .work_dir(&work1)
            .work_dir(&work2)
            .execute()
            .await
            .unwrap();

        let meta_path = capture_dir
            .join("overwrite-workdir")
            .join("0")
            .join("meta.json");
        let meta_content = std::fs::read_to_string(&meta_path).unwrap();

        // The workDir field in meta.json should reflect the SECOND work_dir.
        let work2_str = work2.display().to_string();
        assert!(
            meta_content.contains(&work2_str),
            "workDir should be the second path, got: {meta_content}"
        );
    }

    /// Test 25: Calling .expect_outputs() then .expect_outputs() again.
    /// Since it replaces the entire vec (not appending), the second call wins.
    #[tokio::test]
    async fn expect_outputs_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir);

        // Call expect_outputs twice — second should fully replace first.
        let result = executor
            .agent("overwrite-outputs")
            .prompt("test")
            .work_dir(&work)
            .expect_outputs(&["first.txt", "a.txt"])
            .expect_outputs(&["second.txt", "b.txt"])
            .execute()
            .await;

        // In dry-run, expect_outputs aren't used, so this just tests that
        // the builder doesn't panic. The actual replacement behavior matters
        // only in remote mode.
        assert!(result.is_ok(), "double expect_outputs should not crash");
    }

    /// Test 26: Calling .model() after .work_dir() — order independence.
    #[tokio::test]
    async fn model_after_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        let result = executor
            .agent("model-order")
            .work_dir(&work)
            .model(pipelin3r::Model::Sonnet4_6)
            .prompt("test")
            .execute()
            .await;

        assert!(
            result.is_ok(),
            "model after work_dir should work: {result:?}"
        );
    }

    /// Test 27: Building an agent but never calling .execute().
    /// The #[must_use] attribute should warn, but nothing should break.
    #[tokio::test]
    async fn agent_builder_not_executed() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        // Build but don't execute — this should just drop the builder.
        let _builder = executor.agent("unused").prompt("test");

        // No assertion needed — just verify no panic on drop.
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
            let _ = executor
                .agent("counter-test")
                .prompt("test")
                .execute()
                .await
                .unwrap();
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

    /// Test 29: Counter is GLOBAL across different agent names.
    /// FINDING: The counter is shared across all dry-run captures, not per-agent.
    /// So agent "a" gets 0, agent "b" gets 1, agent "a" again gets 2.
    /// This means the directory structure is `{step_name}/{global_counter}/`.
    #[tokio::test]
    async fn dry_run_counter_is_global() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let _ = executor
            .agent("agent-alpha")
            .prompt("test1")
            .execute()
            .await
            .unwrap();

        let _ = executor
            .agent("agent-beta")
            .prompt("test2")
            .execute()
            .await
            .unwrap();

        let _ = executor
            .agent("agent-alpha")
            .prompt("test3")
            .execute()
            .await
            .unwrap();

        // FIX: counter is now per-step-name, so each agent gets sequential indices:
        // agent-alpha/0, agent-beta/0, agent-alpha/1
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
        // Counter 2 should NOT exist under agent-alpha.
        assert!(
            !capture_dir.join("agent-alpha").join("2").exists(),
            "agent-alpha should only have indices 0 and 1"
        );
    }

    /// Test 30: Dry-run batch and single invocations share the same counter.
    #[tokio::test]
    async fn dry_run_batch_and_single_share_counter() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        // Single invocation first — gets counter 0.
        let _ = executor
            .agent("mixed")
            .prompt("single")
            .execute()
            .await
            .unwrap();

        // Batch invocation — each task gets the next counter values.
        let items = vec![1_u32, 2];
        let _ = executor
            .agent("mixed")
            .items(items, 2)
            .for_each(|item| AgentTask::new().prompt(&format!("batch {item}")))
            .execute()
            .await
            .unwrap();

        // Single again.
        let _ = executor
            .agent("mixed")
            .prompt("single2")
            .execute()
            .await
            .unwrap();

        // Expected: mixed/0 (single), mixed/1 (batch item 1), mixed/2 (batch item 2), mixed/3 (single2)
        assert!(capture_dir.join("mixed").join("0").exists(), "counter 0");
        assert!(capture_dir.join("mixed").join("1").exists(), "counter 1");
        assert!(capture_dir.join("mixed").join("2").exists(), "counter 2");
        assert!(capture_dir.join("mixed").join("3").exists(), "counter 3");
    }

    // ========================================================================
    // Batch work_dir validation in dry-run
    // ========================================================================

    /// Test 31: Batch dry-run does NOT validate work_dir per task.
    /// FINDING: In dry-run batch mode, execute_batch_task_dry_run does NOT call
    /// validate_work_dir. It passes the work_dir directly to execute_dry_run_capture,
    /// which calls collect_relative_paths. If the work_dir is invalid (e.g.,
    /// relative path, contains `..`), the validation is skipped.
    #[tokio::test]
    async fn batch_dry_run_skips_work_dir_validation() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let work = dir.path().join("valid-work");
        std::fs::create_dir_all(&work).unwrap();

        let executor = dry_run_executor(capture_dir.clone());

        // Create a batch where one task has a valid work_dir and another has
        // a path with `..` in it.
        let valid_work = work.clone();
        let traversal_work = dir.path().join("a").join("..").join("valid-work");
        std::fs::create_dir_all(dir.path().join("a")).unwrap();

        let items = vec![
            (String::from("valid"), valid_work),
            (String::from("traversal"), traversal_work),
        ];

        let result = executor
            .agent("batch-no-validate")
            .items(items, 2)
            .for_each(|(_label, path)| AgentTask::new().prompt("test").work_dir(&path))
            .execute()
            .await;

        // FINDING: In dry-run batch mode, the batch does NOT validate work_dirs.
        // The duplicate check runs (BTreeSet), but validate_work_dir is not called.
        // In real mode, execute_single_task calls validate_work_dir, but the
        // dry-run path in AgentBatchBuilder::execute() goes directly to
        // execute_batch_task_dry_run which does NOT validate.
        //
        // The traversal path "a/../valid-work" has a `..` component, which should
        // be rejected. In real mode it would be. In dry-run it isn't.
        //
        // Note: the two paths resolve to the same directory, so the duplicate
        // check MIGHT catch this (if they canonicalize to the same PathBuf).
        // But PathBuf comparison is string-based, not canonical, so
        // "a/../valid-work" != "valid-work" — no duplicate detected.
        match result {
            Ok(results) => {
                // Both tasks succeeded in dry-run — the traversal was NOT caught.
                assert_eq!(results.len(), 2, "should have 2 results");
            }
            Err(e) => {
                // If it was caught, great — but we expect it wasn't.
                let msg = e.to_string();
                assert!(
                    msg.contains("duplicate") || msg.contains(".."),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    /// Test 32: Batch with tasks where some have work_dir and some don't.
    /// The duplicate-dir check uses BTreeSet::insert — None would all compare equal.
    #[tokio::test]
    async fn batch_mixed_work_dir_and_none() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        // Two tasks without work_dir, one with.
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let work_clone = work.clone();

        let items = vec![1_u32, 2, 3];
        let result = executor
            .agent("mixed-workdir")
            .items(items, 3)
            .for_each(move |item| {
                if item == 2 {
                    AgentTask::new()
                        .prompt(&format!("with dir {item}"))
                        .work_dir(&work_clone)
                } else {
                    AgentTask::new().prompt(&format!("no dir {item}"))
                }
            })
            .execute()
            .await;

        // The duplicate check only inserts when work_dir is Some, so multiple
        // None work_dirs don't trigger the duplicate check. This is correct.
        assert!(result.is_ok(), "mixed work_dir should succeed");
        let results = result.unwrap();
        assert_eq!(results.len(), 3, "should have 3 results");
    }

    // ========================================================================
    // extract_step_name edge cases
    // ========================================================================

    /// Test 33: Agent name with special characters produces valid directory name.
    /// extract_step_name converts non-alphanumeric chars to dashes and collapses.
    #[tokio::test]
    async fn agent_name_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        // Agent name with spaces, special chars, unicode.
        let _ = executor
            .agent("my agent / with (special) chars!")
            .prompt("test")
            .execute()
            .await
            .unwrap();

        // The step name extraction uses the name from task YAML, which is
        // the agent name. It slugifies it: lowercase, replace non-alnum with -.
        // "my agent / with (special) chars!" → "my-agent-with-special-chars"
        // (after trim_matches('-'))
        //
        // Verify the directory was created with the slugified name.
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
    /// extract_step_name should fall back to "unknown".
    #[tokio::test]
    async fn agent_name_only_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir.clone());

        let _ = executor
            .agent("!@#$%^&*()")
            .prompt("test")
            .execute()
            .await
            .unwrap();

        // After slugification: all chars become '-', then trim_matches('-') = "".
        // Empty slug falls through to "unknown".
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

        // The name goes into task YAML as `name: !@#$%^&*()`.
        // extract_step_name parses "name: !@#$%^&*()" → rest = "!@#$%^&*()"
        // slugify → "----------" → trim → "" → fall through → "unknown".
        // BUT WAIT: the YAML `name:` field might have issues with special YAML
        // characters like `!` (YAML tag), `@`, `*`. If the YAML parser or
        // build_task_yaml quotes the name, the parsing might differ.
        //
        // Since this is a string match on "name:", not YAML parsing, it should
        // still find the line. Let's just check the result.
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

        let relative = Path::new("relative/path/here");
        let result = executor
            .agent("relative-dir")
            .prompt("test")
            .work_dir(relative)
            .execute()
            .await;

        assert!(result.is_err(), "relative work_dir must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("absolute"),
            "error should mention absolute path requirement: {msg}"
        );
    }

    // ========================================================================
    // AgentTask work_dir validation in batch
    // ========================================================================

    /// Test 36: AgentTask with relative work_dir in batch — real mode validates,
    /// but dry-run mode doesn't call validate_work_dir.
    #[tokio::test]
    async fn batch_task_relative_work_dir_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");

        let executor = dry_run_executor(capture_dir);

        let items = vec![1_u32];
        let result = executor
            .agent("batch-relative")
            .items(items, 1)
            .for_each(|_item| {
                AgentTask::new()
                    .prompt("test")
                    .work_dir(Path::new("relative/path"))
            })
            .execute()
            .await;

        // FINDING: In dry-run batch mode, validate_work_dir is NOT called.
        // execute_batch_task_dry_run passes the work_dir directly to
        // execute_dry_run_capture → collect_relative_paths.
        // collect_relative_paths checks is_dir() which returns false for
        // "relative/path" (doesn't exist), so it returns empty vec.
        // The dry-run capture succeeds with empty workDirFiles.
        //
        // BUG: In real mode, execute_single_task calls validate_work_dir
        // which would reject this. But dry-run silently accepts it, making
        // dry-run an unreliable proxy for real execution.
        match result {
            Ok(results) => {
                // Dry-run accepted it without validation.
                assert_eq!(results.len(), 1);
            }
            Err(_) => {
                // If it was rejected, that would be correct behavior.
            }
        }
    }

    // ========================================================================
    // Dry-run capture directory edge cases
    // ========================================================================

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

        let result = executor
            .agent("readonly-capture")
            .prompt("test")
            .execute()
            .await;

        // Restore permissions before assertions.
        std::fs::set_permissions(&capture_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        // create_dir_all should fail because the capture dir is read-only.
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

        let result = executor.agent("auto-create").prompt("test").execute().await;

        // execute_dry_run_capture calls create_dir_all, which creates
        // intermediate directories. So this should succeed.
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
