use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_error_cases() {
    // Empty string
    assert!(json_repair::<serde_json::Value>("").is_err());

    // Missing colon
    assert!(json_repair::<serde_json::Value>(r#"{"a","#).is_err());

    // Missing object key
    assert!(json_repair::<serde_json::Value>("{:2}").is_err());

    // Unexpected character after valid JSON
    assert!(json_repair::<serde_json::Value>(r#"{"a":2}{}"#).is_err());

    // Invalid unicode
    assert!(json_repair::<serde_json::Value>(r#""\u26""#).is_err());
    assert!(json_repair::<serde_json::Value>(r#""\uZ000""#).is_err());
}

#[test]
fn test_regex_single_slash() {
    // This test case triggers index out of bounds at line 765 and 771 in parse_regex
    // When self.i == 0, accessing self.chars.get(self.i - 1) causes underflow
    // After processing single '/', self.i becomes 2 but chars only has length 1
    let fixture = "/";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("/");
    assert_eq!(actual, expected);
}

#[test]
fn test_regex_with_backslash_slash() {
    // Test regex with escaped slash at the end
    // parse_regex treats the regex as a string literal, so backslash is preserved
    let fixture = r#"/a\/"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(r#"/a\/"#);
    assert_eq!(actual, expected);
}

#[test]
fn test_string_with_colon_at_start() {
    // This test case checks for potential index out of bounds at line 445
    // When self.i == 0 and we try to access self.chars.get(self.i - 1)
    let fixture = ":";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(":");
    assert_eq!(actual, expected);
}

