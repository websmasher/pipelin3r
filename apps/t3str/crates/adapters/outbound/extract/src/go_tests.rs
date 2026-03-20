use super::*;

#[test]
fn finds_test_and_benchmark_functions() {
    let source = r#"
package auth

import "testing"

func TestLogin(t *testing.T) {
    if err := Login("user", "pass"); err != nil {
        t.Fatal(err)
    }
}

func TestLogout(t *testing.T) {
    if err := Logout(); err != nil {
        t.Fatal(err)
    }
}

func BenchmarkLogin(b *testing.B) {
    for i := 0; i < b.N; i++ {
        Login("user", "pass")
    }
}

func helperSetup() {
    // not a test
}
"#;
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 3, "should find 2 tests and 1 benchmark");
    assert_eq!(names.first().map(String::as_str), Some("TestLogin"));
    assert_eq!(names.get(1).map(String::as_str), Some("TestLogout"));
    assert_eq!(names.get(2).map(String::as_str), Some("BenchmarkLogin"));
}

#[test]
fn ignores_non_test_functions() {
    let source = r#"
package main

func main() {
    fmt.Println("hello")
}

func setupDatabase() error {
    return nil
}
"#;
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/auth_test.go")),
        "_test.go suffix"
    );
    assert!(
        is_test_file(Path::new("/repo/pkg/handler_test.go")),
        "nested _test.go"
    );
    assert!(!is_test_file(Path::new("/repo/auth.go")), "regular go file");
    assert!(
        !is_test_file(Path::new("/repo/test_helper.go")),
        "test_ prefix is not Go convention"
    );
}
