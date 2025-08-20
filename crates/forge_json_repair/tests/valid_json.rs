use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_parse_valid_json() {
    // Full JSON object
    let text = r#"{"a":2.3e100,"b":"str","c":null,"d":false,"e":[1,2,3]}"#;
    let actual = json_repair::<serde_json::Value>(text).unwrap();
    let expected = serde_json::from_str::<serde_json::Value>(text).unwrap();
    assert_eq!(actual, expected);

    // Whitespace
    assert_repair("  { \n } \t ");

    // Objects
    assert_repair("{}");
    assert_repair("{  }");
    assert_repair(r#"{"a": {}}"#);
    assert_repair(r#"{"a": "b"}"#);
    assert_repair(r#"{"a": 2}"#);

    // Arrays
    assert_repair("[]");
    assert_repair("[  ]");
    assert_repair("[1,2,3]");
    assert_repair("[ 1 , 2 , 3 ]");
    assert_repair("[1,2,[3,4,5]]");
    assert_repair("[{}]");
    assert_repair(r#"{"a":[]}"#);
    assert_repair(r#"[1, "hi", true, false, null, {}, []]"#);

    // Numbers
    assert_repair("23");
    assert_repair("0");
    assert_repair("0e+2");
    assert_repair("0.0");
    assert_repair("-0");
    assert_repair("2.3");
    assert_repair("2300e3");
    assert_repair("2300e+3");
    assert_repair("2300e-3");
    assert_repair("-2");
    assert_repair("2e-3");
    assert_repair("2.3e-3");

    // Strings
    assert_repair(r#""str""#);
    assert_repair(r#""\"\\\\/\\b\\f\\n\\r\\t""#);
    assert_repair(r#""\\u260E""#);

    // Keywords
    assert_repair("true");
    assert_repair("false");
    assert_repair("null");

    // String delimiters
    assert_repair(r#""""#);
    assert_repair(r#""[""#);
    assert_repair(r#""]""#);
    assert_repair(r#""{""#);
    assert_repair(r#""}""#);
    assert_repair(r#"":""#);
    assert_repair(r#"",""#);
}

// Helper function to assert that input is repaired to itself (valid JSON)
fn assert_repair(text: &str) {
    let actual = json_repair::<serde_json::Value>(text).unwrap();
    let expected = serde_json::from_str::<serde_json::Value>(text).unwrap();
    assert_eq!(actual, expected);
}
