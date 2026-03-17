#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

#[test]
fn creates_and_cleans_up_on_drop() {
    let tmp = tempfile::tempdir().unwrap();
    let path;
    {
        let bundle = BundleDir::new(tmp.path(), "test-slug").unwrap();
        path = bundle.path().to_path_buf();
        assert!(path.exists(), "directory should exist after creation");
        assert!(
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == ".bundle-test-slug"),
            "directory name should be .bundle-test-slug"
        );
    }
    assert!(!path.exists(), "directory should be removed after drop");
}

#[test]
#[allow(clippy::panic, reason = "test: intentionally testing panic cleanup")]
fn cleans_up_on_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let path;
    {
        let bundle = BundleDir::new(tmp.path(), "panic-test").unwrap();
        path = bundle.path().to_path_buf();
        // Write a file inside to verify recursive removal
        crate::fs::write(&path.join("test.txt"), b"hello").unwrap();
        assert!(path.join("test.txt").exists());
        // Simulate panic by catching it
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _b = bundle;
            panic!("simulated panic");
        }));
        assert!(result.is_err(), "panic should have been caught");
    }
    assert!(
        !path.exists(),
        "directory should be removed even after panic"
    );
}

#[test]
fn error_on_invalid_parent() {
    let result = BundleDir::new(Path::new("/nonexistent/path/that/does/not/exist"), "slug");
    assert!(result.is_err(), "should fail for nonexistent parent");
}
