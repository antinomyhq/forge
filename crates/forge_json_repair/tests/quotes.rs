use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_add_missing_quotes() {
    // Simple unquoted strings
    let fixture = "abc";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("abc");
    assert_eq!(actual, expected);

    let fixture = "hello   world";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello   world");
    assert_eq!(actual, expected);

    // Unquoted object keys
    let fixture = "{a:2}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = "{a: 2}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = "{2: 2}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"2": 2});
    assert_eq!(actual, expected);

    let fixture = "{true: 2}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"true": 2});
    assert_eq!(actual, expected);

    // Unquoted array values
    let fixture = "[a,b]";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["a", "b"]);
    assert_eq!(actual, expected);
}

#[test]
fn test_add_missing_end_quote() {
    let fixture = r#""abc"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("abc");
    assert_eq!(actual, expected);

    let fixture = "'abc";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("abc");
    assert_eq!(actual, expected);

    let fixture = r#"{"a":"b}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "b"});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":"b,"c":"d"}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "b", "c": "d"});
    assert_eq!(actual, expected);
}

#[test]
fn test_add_missing_start_quote() {
    let fixture = r#"abc""#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("abc");
    assert_eq!(actual, expected);

    let fixture = r#"[a","b"]"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["a", "b"]);
    assert_eq!(actual, expected);

    let fixture = r#"{"a":"foo","b":"bar"}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo", "b": "bar"});
    assert_eq!(actual, expected);

    let fixture = r#"{a":"foo","b":"bar"}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo", "b": "bar"});
    assert_eq!(actual, expected);
}

#[test]
fn test_replace_single_quotes() {
    let fixture = "{'a':2}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = "{'a':'foo'}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo"});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":'foo'}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo"});
    assert_eq!(actual, expected);

    let fixture = "{a:'foo',b:'bar'}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo", "b": "bar"});
    assert_eq!(actual, expected);
}
