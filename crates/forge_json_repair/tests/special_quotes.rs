use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_escaped_string_contents() {
    let fixture = r#"\"hello world\""#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello world");
    assert_eq!(actual, expected);

    let fixture = r#"\"hello world\"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello world");
    assert_eq!(actual, expected);

    let fixture = r#"\"hello \"world\"\""#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello \"world\"");
    assert_eq!(actual, expected);

    let fixture = r#"[\"hello \"world\"\"]"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["hello \"world\""]);
    assert_eq!(actual, expected);

    let fixture = r#"{\"stringified\": \"hello \"world\"\"}"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"stringified": "hello \"world\""});
    assert_eq!(actual, expected);

    // Weird but close to likely intention
    let fixture = r#"[\"hello\, \"world\"]"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["hello, \"world"]);
    assert_eq!(actual, expected);

    // Invalid end quote handling
    let fixture = r#"\"hello""#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("hello");
    assert_eq!(actual, expected);
}

#[test]
fn test_special_quote_characters() {
    // Left/right single quotes (using unicode escapes)
    let fixture = "\u{2018}foo\u{2019}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo");
    assert_eq!(actual, expected);

    // Left/right double quotes (using unicode escapes)
    let fixture = "\u{201C}foo\u{201D}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo");
    assert_eq!(actual, expected);

    // Backtick and acute accent
    let fixture = "\u{0060}foo\u{00B4}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo");
    assert_eq!(actual, expected);

    // Mixed quotes
    let fixture = "\u{0060}foo'";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("foo");
    assert_eq!(actual, expected);

    // In object syntax
    let fixture = "{\u{2018}a\u{2019}:\u{2018}b\u{2019}}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "b"});
    assert_eq!(actual, expected);

    let fixture = "{\u{0060}a\u{00B4}:\u{0060}b\u{00B4}}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "b"});
    assert_eq!(actual, expected);
}

#[test]
fn test_special_quotes_inside_strings() {
    // Should not replace special quotes inside normal strings
    let fixture = "\"Rounded \u{201C} quote\"";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("Rounded \u{201C} quote");
    assert_eq!(actual, expected);

    let fixture = "'\u{201C}Rounded quote'";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("\u{201C}Rounded quote");
    assert_eq!(actual, expected);

    let fixture = "\"Rounded \u{2018} quote\"";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("Rounded \u{2018} quote");
    assert_eq!(actual, expected);

    let fixture = "'\u{2018}Rounded quote'";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("\u{2018}Rounded quote");
    assert_eq!(actual, expected);

    let fixture = r#"'Double " quote'"#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("Double \" quote");
    assert_eq!(actual, expected);
}

#[test]
fn test_quote_repair_edge_cases() {
    // Should leave string content untouched
    let fixture = r#""{a:b}""#;
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("{a:b}");
    assert_eq!(actual, expected);
}

#[test]
fn test_special_whitespace_characters() {
    // Non-breaking space and other special spaces
    let fixture = "{\"a\":\u{00a0}\"foo\u{00a0}bar\"}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo\u{00a0}bar"});
    assert_eq!(actual, expected);

    let fixture = "{\"a\":\u{202F}\"foo\"}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo"});
    assert_eq!(actual, expected);

    let fixture = "{\"a\":\u{205F}\"foo\"}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo"});
    assert_eq!(actual, expected);

    let fixture = "{\"a\":\u{3000}\"foo\"}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"a": "foo"});
    assert_eq!(actual, expected);
}

#[test]
fn test_stop_at_newline_for_missing_quotes() {
    let fixture = "[\n\"abc,\n\"def\"\n]";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["abc", "def"]);
    assert_eq!(actual, expected);

    let fixture = "[\n\"abc,  \n\"def\"\n]";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["abc", "def"]);
    assert_eq!(actual, expected);

    let fixture = "[\"abc]\n";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["abc"]);
    assert_eq!(actual, expected);

    let fixture = "[\"abc  ]\n";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["abc"]);
    assert_eq!(actual, expected);

    let fixture = "[\n[\n\"abc\n]\n]\n";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!([["abc"]]);
    assert_eq!(actual, expected);
}
