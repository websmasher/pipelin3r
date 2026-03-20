use super::*;

#[test]
fn finds_top_level_test_functions() {
    let source = r"
import pytest

def test_login():
    assert True

def test_logout():
    assert True

def helper_setup():
    pass
";
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 test functions");
    assert_eq!(names.first().map(String::as_str), Some("test_login"));
    assert_eq!(names.get(1).map(String::as_str), Some("test_logout"));
}

#[test]
fn finds_class_methods() {
    let source = r"
import unittest

class TestAuth(unittest.TestCase):
    def test_valid_token(self):
        self.assertTrue(True)

    def test_expired_token(self):
        self.assertFalse(False)

    def setUp(self):
        pass
";
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 test methods in class");
    assert_eq!(names.first().map(String::as_str), Some("test_valid_token"));
    assert_eq!(names.get(1).map(String::as_str), Some("test_expired_token"));
}

#[test]
fn ignores_non_test_functions() {
    let source = r#"
def setup():
    pass

def create_user(name):
    return {"name": name}

class Helper:
    def do_thing(self):
        pass
"#;
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/test_auth.py")),
        "test_ prefix"
    );
    assert!(
        is_test_file(Path::new("/repo/auth_test.py")),
        "_test suffix"
    );
    assert!(!is_test_file(Path::new("/repo/auth.py")), "regular module");
    assert!(
        !is_test_file(Path::new("/repo/conftest.py")),
        "conftest is not a test file"
    );
}
