use forge_json_repair::jsonrepair;
use pretty_assertions::assert_eq;

#[test]
fn test_remove_comments() {
    let fixture = r#"/* foo */ {}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);

    let fixture = r#"{} /* foo */"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":"foo",/*hello*/"b":"bar"}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo", "b": "bar"});
    assert_eq!(actual, expected);

    let fixture = r#"{} // comment"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);

    // Should not remove comments inside strings
    let fixture = r#""/* foo */""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("/* foo */");
    assert_eq!(actual, expected);
}

#[test]
fn test_unicode_support() {
    // Unicode characters in strings
    let fixture = r#""★""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("★");
    assert_eq!(actual, expected);

    let fixture = r#""😀""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("😀");
    assert_eq!(actual, expected);

    // Escaped unicode
    let fixture = r#""\\u2605""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("\\u2605");
    assert_eq!(actual, expected);

    // Unicode in keys
    let fixture = r#"{"★":true}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"★": true});
    assert_eq!(actual, expected);
}
