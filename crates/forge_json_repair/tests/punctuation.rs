use forge_json_repair::jsonrepair;
use pretty_assertions::assert_eq;

#[test]
fn test_repair_missing_commas() {
    // Array items
    let fixture = r#"{"array": [{}{}]}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": [{}, {}]});
    assert_eq!(actual, expected);

    // Object properties
    let fixture = r#"{"a":2"b":3}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2, "b": 3});
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_missing_colons() {
    let fixture = r#"{"a" "b"}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "b"});
    assert_eq!(actual, expected);

    let fixture = r#"{"a" 2}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = r#"{"a" true}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": true});
    assert_eq!(actual, expected);

    let fixture = r#"{"a" false}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": false});
    assert_eq!(actual, expected);

    let fixture = r#"{"a" null}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": null});
    assert_eq!(actual, expected);
}

#[test]
fn test_strip_leading_commas() {
    let fixture = "[,1,2,3]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = r#"{,"message": "hi"}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"message": "hi"});
    assert_eq!(actual, expected);
}

#[test]
fn test_strip_trailing_commas() {
    let fixture = "[1,2,3,]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2,}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = "4,";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(4);
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2},"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);
}
