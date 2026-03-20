use super::*;

#[test]
fn apply_filter_no_filter() {
    let funcs = vec![String::from("test_a"), String::from("test_b")];
    let (result, relevant) = apply_filter(&funcs, None);
    assert_eq!(result.len(), 2, "all functions returned without filter");
    assert!(relevant, "relevant when no filter");
}

#[test]
fn apply_filter_with_match() {
    let funcs = vec![String::from("test_auth"), String::from("test_db")];
    let (result, relevant) = apply_filter(&funcs, Some("auth"));
    assert_eq!(result.len(), 1, "only matching function returned");
    assert_eq!(result.first().map(String::as_str), Some("test_auth"));
    assert!(relevant, "relevant when filter matches");
}

#[test]
fn apply_filter_no_match() {
    let funcs = vec![String::from("test_auth")];
    let (result, relevant) = apply_filter(&funcs, Some("db"));
    assert!(result.is_empty(), "no matches");
    assert!(!relevant, "not relevant when no match");
}

#[test]
fn matches_extension_check() {
    assert!(
        matches_extension(Path::new("foo.py"), &["py", "pyi"]),
        "py matches"
    );
    assert!(
        !matches_extension(Path::new("foo.rs"), &["py"]),
        "rs does not match py"
    );
    assert!(
        !matches_extension(Path::new("no_ext"), &["py"]),
        "no extension"
    );
}

#[test]
fn skip_dirs() {
    assert!(should_skip_dir(Path::new("/repo/.git")), ".git skipped");
    assert!(
        should_skip_dir(Path::new("/repo/node_modules")),
        "node_modules skipped"
    );
    assert!(
        should_skip_dir(Path::new("/repo/.hidden")),
        "dotdir skipped"
    );
    assert!(!should_skip_dir(Path::new("/repo/src")), "src not skipped");
}
