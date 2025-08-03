use forge_json_repair::jsonrepair;
use pretty_assertions::assert_eq;

#[test]
fn test_escape_characters() {
    let fixture = r#""foo'bar""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo'bar");
    assert_eq!(actual, expected);

    let fixture = r#""foo\"bar""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo\"bar");
    assert_eq!(actual, expected);

    let fixture = "'foo\"bar'";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo\"bar");
    assert_eq!(actual, expected);

    let fixture = "'foo\\'bar'";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo'bar");
    assert_eq!(actual, expected);
}

#[test]
fn test_escape_control_characters() {
    let fixture = "\"hello\nworld\"";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello\nworld");
    assert_eq!(actual, expected);

    let fixture = "\"hello\tworld\"";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello\tworld");
    assert_eq!(actual, expected);

    let fixture = "\"hello\rworld\"";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello\rworld");
    assert_eq!(actual, expected);
}

#[test]
fn test_escape_unescaped_quotes() {
    let fixture = r#""The TV has a 24" screen""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("The TV has a 24\" screen");
    assert_eq!(actual, expected);

    let fixture = r#"{"key": "apple "bee" carrot"}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"key": "apple \"bee\" carrot"});
    assert_eq!(actual, expected);
}
