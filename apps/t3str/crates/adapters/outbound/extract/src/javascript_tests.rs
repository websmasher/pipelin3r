use super::*;

#[test]
fn finds_jest_test_calls() {
    let source = r#"
describe("auth", () => {
    test("should login successfully", () => {
        expect(login("user", "pass")).toBeTruthy();
    });

    test("should reject bad password", () => {
        expect(login("user", "wrong")).toBeFalsy();
    });
});
"#;
    let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 test() calls");
    assert_eq!(
        names.first().map(String::as_str),
        Some("should login successfully")
    );
    assert_eq!(
        names.get(1).map(String::as_str),
        Some("should reject bad password")
    );
}

#[test]
fn finds_mocha_it_calls() {
    let source = r#"
describe("calculator", function() {
    it("adds numbers", function() {
        assert.equal(add(2, 2), 4);
    });

    it("subtracts numbers", function() {
        assert.equal(subtract(4, 2), 2);
    });
});
"#;
    let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 it() calls");
    assert_eq!(names.first().map(String::as_str), Some("adds numbers"));
    assert_eq!(names.get(1).map(String::as_str), Some("subtracts numbers"));
}

#[test]
fn ignores_non_test_calls() {
    let source = r#"
const result = add(2, 3);
console.log("hello");
describe("group", () => {});
"#;
    let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no test functions");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/src/auth.test.js")),
        "auth.test.js is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/src/auth.spec.ts")),
        "auth.spec.ts is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/__tests__/auth.js")),
        "file in __tests__/ dir"
    );
    assert!(
        is_test_file(Path::new("/repo/test/utils.js")),
        "file in test/ dir"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/auth.js")),
        "auth.js is not a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/index.ts")),
        "index.ts is not a test file"
    );
}
