use forge_json_repair::json_repair;
use pretty_assertions::assert_eq;

#[test]
fn test_invalid_numbers_to_strings() {
    let fixture = "ES2020";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("ES2020");
    assert_eq!(actual, expected);

    let fixture = "0.0.1";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("0.0.1");
    assert_eq!(actual, expected);

    let fixture = "746de9ad-d4ff-4c66-97d7-00a92ad46967";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("746de9ad-d4ff-4c66-97d7-00a92ad46967");
    assert_eq!(actual, expected);

    let fixture = "234..5";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("234..5");
    assert_eq!(actual, expected);

    let fixture = "2e3.4";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("2e3.4");
    assert_eq!(actual, expected);
}

#[test]
fn test_numbers_with_leading_zeros() {
    let fixture = "0789";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("0789");
    assert_eq!(actual, expected);

    let fixture = "000789";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("000789");
    assert_eq!(actual, expected);

    let fixture = "001.2";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!("001.2");
    assert_eq!(actual, expected);

    let fixture = "[0789]";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!(["0789"]);
    assert_eq!(actual, expected);

    let fixture = "{value:0789}";
    let actual = json_repair::<serde_json::Value>(fixture).unwrap();
    let expected = serde_json::json!({"value": "0789"});
    assert_eq!(actual, expected);
}
