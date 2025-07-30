use forge_json_repair::jsonrepair;
use pretty_assertions::assert_eq;

#[test]
fn test_add_missing_closing_brackets() {
    let fixture = "{";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2,"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = "[";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([]);
    assert_eq!(actual, expected);

    let fixture = "[1,2,3";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = "[1,2,3,";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_truncated_json() {
    let fixture = r#""foo"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo");
    assert_eq!(actual, expected);

    let fixture = "[";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([]);
    assert_eq!(actual, expected);

    let fixture = r#"["foo"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["foo"]);
    assert_eq!(actual, expected);

    let fixture = r#"["foo","#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["foo"]);
    assert_eq!(actual, expected);

    let fixture = r#"{"foo":"bar""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"foo": "bar"});
    assert_eq!(actual, expected);

    let fixture = r#"{"foo":"bar"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"foo": "bar"});
    assert_eq!(actual, expected);

    let fixture = r#"{"foo":"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"foo": null});
    assert_eq!(actual, expected);

    let fixture = r#"{"foo""#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"foo": null});
    assert_eq!(actual, expected);

    let fixture = r#"{"foo"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"foo": null});
    assert_eq!(actual, expected);

    let fixture = "{";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);

    let fixture = "2.";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(2.0);
    assert_eq!(actual, expected);

    let fixture = "2e";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(2e0);
    assert_eq!(actual, expected);

    let fixture = "2e+";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(2e0);
    assert_eq!(actual, expected);

    let fixture = "2e-";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(2e0);
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_ellipsis() {
    // Arrays
    let fixture = "[1,2,3,...]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = "[1,2,3,...,9]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3, 9]);
    assert_eq!(actual, expected);

    let fixture = "[...,7,8,9]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([7, 8, 9]);
    assert_eq!(actual, expected);

    let fixture = "[...]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([]);
    assert_eq!(actual, expected);

    // Objects
    let fixture = r#"{"a":2,"b":3,...}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2, "b": 3});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2,"b":3,...,"z":26}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2, "b": 3, "z": 26});
    assert_eq!(actual, expected);

    let fixture = "{...}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);
}
