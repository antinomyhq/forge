use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_parse_unquoted_strings() {
    let fixture = "hello world";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello world");
    assert_eq!(actual, expected);

    let fixture = "She said: no way";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("She said: no way");
    assert_eq!(actual, expected);
}

#[test]
fn test_turn_symbols_into_strings() {
    let fixture = "foo";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo");
    assert_eq!(actual, expected);

    let fixture = "[1,foo,4]";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, "foo", 4]);
    assert_eq!(actual, expected);

    let fixture = "{foo: bar}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"foo": "bar"});
    assert_eq!(actual, expected);

    let fixture = "foo 2 bar";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo 2 bar");
    assert_eq!(actual, expected);

    let fixture = "{greeting: hello world}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"greeting": "hello world"});
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_urls() {
    let fixture = "https://www.example.com/";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("https://www.example.com/");
    assert_eq!(actual, expected);

    let fixture = "{url:https://www.example.com/}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"url": "https://www.example.com/"});
    assert_eq!(actual, expected);

    let fixture = r#"{"url":"https://www.example.com/}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"url": "https://www.example.com/"});
    assert_eq!(actual, expected);
}
