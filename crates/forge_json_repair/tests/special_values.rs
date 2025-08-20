use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_python_constants() {
    let fixture = "True";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(true);
    assert_eq!(actual, expected);

    let fixture = "False";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(false);
    assert_eq!(actual, expected);

    let fixture = "None";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(null);
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_undefined_values() {
    let fixture = r#"{"a":undefined}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": null});
    assert_eq!(actual, expected);

    let fixture = "[undefined]";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([null]);
    assert_eq!(actual, expected);

    let fixture = "undefined";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(null);
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_missing_object_value() {
    let fixture = r#"{"a":}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": null});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":,"b":2}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": null, "b": 2});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": null});
    assert_eq!(actual, expected);
}
