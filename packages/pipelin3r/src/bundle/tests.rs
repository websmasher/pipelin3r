use super::*;

#[test]
fn validate_path_normal() {
    assert!(
        validate_path("file.txt").is_ok(),
        "simple filename should be valid"
    );
    assert!(
        validate_path("dir/file.txt").is_ok(),
        "nested path should be valid"
    );
    assert!(
        validate_path("a/b/c/d.json").is_ok(),
        "deeply nested path should be valid"
    );
}

#[test]
fn validate_path_rejects_empty() {
    assert!(validate_path("").is_err(), "empty path should be rejected");
}

#[test]
fn validate_path_rejects_traversal() {
    assert!(
        validate_path("../etc/passwd").is_err(),
        "parent traversal should be rejected"
    );
    assert!(
        validate_path("a/../../b").is_err(),
        "double parent traversal should be rejected"
    );
}

#[test]
fn validate_path_rejects_absolute() {
    assert!(
        validate_path("/etc/passwd").is_err(),
        "absolute path should be rejected"
    );
}

#[test]
fn validate_path_rejects_current_dir() {
    assert!(
        validate_path("./file.txt").is_err(),
        "current dir prefix should be rejected"
    );
}
