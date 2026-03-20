use super::*;

#[test]
fn finds_rspec_it_blocks() {
    let source = r#"
RSpec.describe User do
  it "validates email format" do
    expect(user.email).to be_valid
  end

  it "requires a name" do
    expect(user.name).not_to be_nil
  end

  def helper_method
    "not a test"
  end
end
"#;
    let lang: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 RSpec it blocks");
    assert_eq!(
        names.first().map(String::as_str),
        Some("validates email format")
    );
    assert_eq!(names.get(1).map(String::as_str), Some("requires a name"));
}

#[test]
fn finds_minitest_methods() {
    let source = r#"
class UserTest < Minitest::Test
  def test_login_success
    assert user.login("password")
  end

  def test_login_failure
    refute user.login("wrong")
  end

  def setup
    @user = User.new
  end
end
"#;
    let lang: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 Minitest test_ methods");
    assert_eq!(
        names.first().map(String::as_str),
        Some("test_login_success")
    );
    assert_eq!(names.get(1).map(String::as_str), Some("test_login_failure"));
}

#[test]
fn ignores_non_test_methods() {
    let source = r"
class Calculator
  def add(a, b)
    a + b
  end

  def subtract(a, b)
    a - b
  end
end
";
    let lang: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/spec/user_spec.rb")),
        "_spec.rb is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/test/user_test.rb")),
        "_test.rb is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/test/test_helper.rb")),
        "test_*.rb is a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/app/models/user.rb")),
        "regular file is not a test file"
    );
}
