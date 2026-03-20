use super::*;

#[test]
fn finds_test_methods() {
    let source = r"<?php
class UserTest extends TestCase {
    public function testCreateUser() {
        $this->assertTrue(true);
    }

    public function testDeleteUser() {
        $this->assertFalse(false);
    }

    public function helperMethod() {
        return 42;
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_php::LANGUAGE_PHP.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 test methods");
    assert_eq!(names.first().map(String::as_str), Some("testCreateUser"));
    assert_eq!(names.get(1).map(String::as_str), Some("testDeleteUser"));
}

#[test]
fn ignores_non_test_methods() {
    let source = r"<?php
class UserService {
    public function createUser($name) {
        return new User($name);
    }

    public function getUser($id) {
        return User::find($id);
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_php::LANGUAGE_PHP.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests in non-test class");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/tests/UserTest.php")),
        "UserTest.php is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/tests/test_basic.phpt")),
        "test_basic.phpt is a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/User.php")),
        "User.php is not a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/TestHelper.php")),
        "TestHelper.php does not end with Test.php stem"
    );
    assert!(
        !is_test_file(Path::new("/repo/basic.phpt")),
        "basic.phpt does not start with test"
    );
}
