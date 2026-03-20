use super::*;

#[test]
fn finds_test_blocks() {
    let source = r#"
defmodule UserTest do
  use ExUnit.Case

  test "creates a new user" do
    assert {:ok, _user} = User.create(%{name: "Alice"})
  end

  test "rejects invalid email" do
    assert {:error, _} = User.create(%{email: "bad"})
  end

  def helper do
    :ok
  end
end
"#;
    let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 test blocks");
    assert_eq!(
        names.first().map(String::as_str),
        Some("creates a new user")
    );
    assert_eq!(
        names.get(1).map(String::as_str),
        Some("rejects invalid email")
    );
}

#[test]
fn ignores_non_test_functions() {
    let source = r"
defmodule Calculator do
  def add(a, b) do
    a + b
  end

  def subtract(a, b) do
    a - b
  end
end
";
    let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/test/user_test.exs")),
        "_test.exs is a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/test/test_helper.exs")),
        "test_helper.exs is not a _test.exs file"
    );
    assert!(
        !is_test_file(Path::new("/repo/lib/user.ex")),
        ".ex file is not a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/lib/user_test.ex")),
        "_test.ex is not .exs so not a test file"
    );
}
