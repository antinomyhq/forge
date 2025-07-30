use forge_json_repair::jsonrepair;
use pretty_assertions::assert_eq;

#[test]
fn test_newline_separated_json() {
    // Basic newline separated JSON (like MongoDB output)
    let fixture = "/* 1 */\n{}\n\n/* 2 */\n{}\n\n/* 3 */\n{}\n";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([{}, {}, {}]);
    assert_eq!(actual, expected);

    // With existing commas
    let fixture = "/* 1 */\n{},\n\n/* 2 */\n{},\n\n/* 3 */\n{}\n";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([{}, {}, {}]);
    assert_eq!(actual, expected);

    // With trailing comma
    let fixture = "/* 1 */\n{},\n\n/* 2 */\n{},\n\n/* 3 */\n{},\n";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([{}, {}, {}]);
    assert_eq!(actual, expected);
}

#[test]
fn test_comma_separated_lists() {
    let fixture = "1,2,3";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = "1,2,3,";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = "1\n2\n3";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2, 3]);
    assert_eq!(actual, expected);

    let fixture = "a\nb";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["a", "b"]);
    assert_eq!(actual, expected);

    let fixture = "a,b";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["a", "b"]);
    assert_eq!(actual, expected);
}

#[test]
fn test_missing_comma_repairs() {
    // Missing commas between array items
    let fixture = r#"{"array": [{}{}]}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": [{}, {}]});
    assert_eq!(actual, expected);

    let fixture = r#"{"array": [{} {}]}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": [{}, {}]});
    assert_eq!(actual, expected);

    let fixture = "{\n\"array\": [{}\n{}]\n}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": [{}, {}]});
    assert_eq!(actual, expected);

    let fixture = "{\n\"array\": [\n{}\n{}\n]\n}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": [{}, {}]});
    assert_eq!(actual, expected);

    let fixture = "{\n\"array\": [\n1\n2\n]\n}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": [1, 2]});
    assert_eq!(actual, expected);

    let fixture = "{\n\"array\": [\n\"a\"\n\"b\"\n]\n}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": ["a", "b"]});
    assert_eq!(actual, expected);

    // Should leave normal array as is
    let fixture = "[\n{},\n{}\n]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([{}, {}]);
    assert_eq!(actual, expected);
}

#[test]
fn test_missing_comma_between_object_properties() {
    let fixture = "{\"a\":2\n\"b\":3\n}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2, "b": 3});
    assert_eq!(actual, expected);

    let fixture = "{\"a\":2\n\"b\":3\n\"c\":4}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2, "b": 3, "c": 4});
    assert_eq!(actual, expected);

    let fixture = "{\n  \"firstName\": \"John\"\n  \"lastName\": \"Smith\"}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"firstName": "John", "lastName": "Smith"});
    assert_eq!(actual, expected);

    let fixture = "{\n  \"firstName\": \"John\" /* comment */ \n  \"lastName\": \"Smith\"}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"firstName": "John", "lastName": "Smith"});
    assert_eq!(actual, expected);

    // Verify parsing a comma after a return
    let fixture = "{\n  \"firstName\": \"John\"\n  ,  \"lastName\": \"Smith\"}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"firstName": "John", "lastName": "Smith"});
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_combination_of_issues() {
    let fixture = "{\n\"array\": [\na\nb\n]\n}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"array": ["a", "b"]});
    assert_eq!(actual, expected);

    let fixture = "1\n2";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([1, 2]);
    assert_eq!(actual, expected);

    let fixture = "[a,b\nc]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["a", "b", "c"]);
    assert_eq!(actual, expected);
}

#[test]
fn test_repair_numbers_at_end() {
    let fixture = r#"{"a":2.}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2.0});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2e}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2e0});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2e-}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2e0});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":-}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": -0.0});
    assert_eq!(actual, expected);

    let fixture = "[2e,]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([2e0]);
    assert_eq!(actual, expected);

    let fixture = "[2e ]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([2e0]);
    assert_eq!(actual, expected);

    let fixture = "[-,]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([-0.0]);
    assert_eq!(actual, expected);
}

#[test]
fn test_remove_redundant_closing_brackets() {
    let fixture = r#"{"a": 1}}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 1});
    assert_eq!(actual, expected);

    let fixture = r#"{"a": 1}}]}}"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 1});
    assert_eq!(actual, expected);

    let fixture = r#"{"a": 1 }  }  ]  }  "#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 1});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2]"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = r#"{"a":2,]"#;
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": 2});
    assert_eq!(actual, expected);

    let fixture = "{}}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);

    let fixture = "[2,}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([2]);
    assert_eq!(actual, expected);

    let fixture = "[}";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([]);
    assert_eq!(actual, expected);

    let fixture = "{]";
    let actual = jsonrepair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({});
    assert_eq!(actual, expected);
}
