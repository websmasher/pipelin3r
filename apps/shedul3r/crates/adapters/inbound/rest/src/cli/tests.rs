use super::*;

#[test]
fn parse_env_vars_empty() {
    assert!(
        parse_env_vars(&[]).is_none(),
        "empty vec should return None"
    );
}

#[test]
fn parse_env_vars_single() {
    let vars = vec!["KEY=value".to_owned()];
    let map = parse_env_vars(&vars);
    assert!(map.is_some(), "should parse one var");
    let m = map.unwrap_or_default();
    assert_eq!(m.get("KEY").map(String::as_str), Some("value"));
}

#[test]
fn parse_env_vars_value_with_equals() {
    let vars = vec!["DB_URL=postgres://host=foo".to_owned()];
    let map = parse_env_vars(&vars);
    assert!(map.is_some(), "should handle = in value");
    let m = map.unwrap_or_default();
    assert_eq!(
        m.get("DB_URL").map(String::as_str),
        Some("postgres://host=foo")
    );
}

#[test]
fn parse_env_vars_no_equals_skipped() {
    let vars = vec!["NOEQUALS".to_owned()];
    assert!(parse_env_vars(&vars).is_none(), "no = should be skipped");
}
