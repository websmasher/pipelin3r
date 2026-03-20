use super::*;

#[test]
fn finds_basic_test_functions() {
    let source = r"
#[cfg(test)]
mod tests {
    #[test]
    fn test_addition() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_subtraction() {
        assert_eq!(4 - 2, 2);
    }

    fn helper() -> i32 {
        42
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 test functions");
    assert_eq!(names.first().map(String::as_str), Some("test_addition"));
    assert_eq!(names.get(1).map(String::as_str), Some("test_subtraction"));
}

#[test]
fn finds_tokio_test_functions() {
    let source = r"
#[tokio::test]
async fn test_async_operation() {
    let result = do_something().await;
    assert!(result.is_ok());
}

#[test]
fn test_sync() {
    assert!(true);
}
";
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find both tokio and regular tests");
    assert_eq!(
        names.first().map(String::as_str),
        Some("test_async_operation")
    );
    assert_eq!(names.get(1).map(String::as_str), Some("test_sync"));
}

#[test]
fn ignores_non_test_functions() {
    let source = r#"
fn regular_function() {
    println!("not a test");
}

pub fn another_function() -> bool {
    true
}
"#;
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/tests/integration.rs")),
        "file in tests/ dir"
    );
    assert!(
        is_test_file(Path::new("/repo/src/test_utils.rs")),
        "filename contains test"
    );
    assert!(
        is_test_file(Path::new("/repo/src/main.rs")),
        "src/ files are scanned for inline #[cfg(test)] modules"
    );
    assert!(
        is_test_file(Path::new("/repo/src/lib.rs")),
        "src/ files are scanned for inline #[cfg(test)] modules"
    );
    assert!(
        !is_test_file(Path::new("/repo/examples/demo.rs")),
        "examples/ files are not test files"
    );
}
