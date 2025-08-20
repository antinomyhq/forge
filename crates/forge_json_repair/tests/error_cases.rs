use forge_json_repair::json_repair;

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
