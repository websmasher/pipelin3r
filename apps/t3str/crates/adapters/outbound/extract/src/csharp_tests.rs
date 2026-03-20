use super::*;

#[test]
fn finds_nunit_test_methods() {
    let source = r"
using NUnit.Framework;

[TestFixture]
public class CalculatorTest {
    [Test]
    public void AddNumbers() {
        Assert.AreEqual(4, Calculator.Add(2, 2));
    }

    [Test]
    public void SubtractNumbers() {
        Assert.AreEqual(2, Calculator.Subtract(4, 2));
    }

    public void HelperMethod() {
        // not a test
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 NUnit test methods");
    assert_eq!(names.first().map(String::as_str), Some("AddNumbers"));
    assert_eq!(names.get(1).map(String::as_str), Some("SubtractNumbers"));
}

#[test]
fn finds_xunit_fact_methods() {
    let source = r"
using Xunit;

public class AuthTests {
    [Fact]
    public void LoginSucceeds() {
        Assert.True(Auth.Login());
    }

    [Fact]
    public void LoginFails() {
        Assert.False(Auth.BadLogin());
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 2, "should find 2 xUnit Fact methods");
    assert_eq!(names.first().map(String::as_str), Some("LoginSucceeds"));
    assert_eq!(names.get(1).map(String::as_str), Some("LoginFails"));
}

#[test]
fn finds_mstest_methods() {
    let source = r"
using Microsoft.VisualStudio.TestTools.UnitTesting;

[TestClass]
public class OrderTest {
    [TestMethod]
    public void CreateOrder() {
        var order = new Order();
        Assert.IsNotNull(order);
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert_eq!(names.len(), 1, "should find 1 MSTest method");
    assert_eq!(names.first().map(String::as_str), Some("CreateOrder"));
}

#[test]
fn ignores_non_test_methods() {
    let source = r"
public class UserService {
    public void CreateUser(string name) {
        // business logic
    }

    public User GetUser(int id) {
        return null;
    }
}
";
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    let names = helpers::extract_with_query(source, &lang, QUERY).unwrap_or_default();
    assert!(names.is_empty(), "should find no tests in non-test class");
}

#[test]
fn is_test_file_checks() {
    assert!(
        is_test_file(Path::new("/repo/tests/UserTest.cs")),
        "UserTest.cs is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/tests/AuthTests.cs")),
        "AuthTests.cs is a test file"
    );
    assert!(
        is_test_file(Path::new("/repo/tests/TestHelpers.cs")),
        "TestHelpers.cs starts with Test"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/User.cs")),
        "User.cs is not a test file"
    );
    assert!(
        !is_test_file(Path::new("/repo/src/Contest.cs")),
        "Contest.cs is not a test file"
    );
}
