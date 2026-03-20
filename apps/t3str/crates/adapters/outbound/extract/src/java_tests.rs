use super::*;

#[test]
fn finds_junit_test_methods() {
    let source = r#"
import org.junit.jupiter.api.Test;

class UserServiceTest {
    @Test
    void shouldCreateUser() {
        User user = service.create("Alice");
        assertNotNull(user);
    }

    @Test
    void shouldDeleteUser() {
        service.delete(1);
        assertNull(service.find(1));
    }

    void helperMethod() {
        // not a test
    }
}
"#;
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 @Test methods");
    assert_eq!(names.first().map(String::as_str), Some("shouldCreateUser"));
    assert_eq!(names.get(1).map(String::as_str), Some("shouldDeleteUser"));
}

#[test]
fn ignores_methods_without_test_annotation() {
    let source = r"
class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public int subtract(int a, int b) {
        return a - b;
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/src/test/UserServiceTest.java")),
        "ends with Test.java"
    );
    assert!(
        is_test_file(Path::new("/repo/src/test/UserTests.java")),
        "ends with Tests.java"
    );
    assert!(
        is_test_file(Path::new("/repo/src/test/TestUserService.java")),
        "starts with Test"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/main/UserService.java")),
        "regular file is not a test file"
    );
}
