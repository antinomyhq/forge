use std::collections::BTreeMap;

use forge_json_repair::json_repair;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallArguments {
    Unparsed(String),
    Parsed(Value),
}

impl Serialize for ToolCallArguments {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ToolCallArguments::Unparsed(value) => {
                // Use RawValue to serialize the JSON string without double serialization
                match RawValue::from_string(value.clone()) {
                    Ok(raw) => raw.serialize(serializer),
                    Err(_) => value.serialize(serializer), // Fallback if not valid JSON
                }
            }
            ToolCallArguments::Parsed(value) => value.serialize(serializer),
        }
    }
}
impl<'de> Deserialize<'de> for ToolCallArguments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Value::deserialize(deserializer)?.into())
    }
}

impl Default for ToolCallArguments {
    fn default() -> Self {
        ToolCallArguments::Parsed(Value::Object(Map::new()))
    }
}

impl ToolCallArguments {
    pub fn into_string(self) -> String {
        match self {
            ToolCallArguments::Unparsed(str) => str,
            ToolCallArguments::Parsed(value) => value.to_string(),
        }
    }

    pub fn parse(&self) -> Result<Value, Error> {
        match self {
            ToolCallArguments::Unparsed(json) => {
                Ok(
                    json_repair(json).map_err(|error| crate::Error::ToolCallArgument {
                        error,
                        args: json.to_owned(),
                    })?,
                )
            }
            ToolCallArguments::Parsed(value) => Ok(value.to_owned()),
        }
    }

    pub fn from_json(str: &str) -> Self {
        ToolCallArguments::Unparsed(str.to_string())
    }

    pub fn from_parameters(object: BTreeMap<String, String>) -> ToolCallArguments {
        let mut map = Map::new();

        for (key, value) in object {
            map.insert(key, convert_string_to_value(&value));
        }

        ToolCallArguments::Parsed(Value::Object(map))
    }
}

fn convert_string_to_value(value: &str) -> Value {
    // Try to parse as boolean first
    match value.trim().to_lowercase().as_str() {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }

    // Try to parse as number
    if let Ok(int_val) = value.parse::<i64>() {
        return Value::Number(int_val.into());
    }

    if let Ok(float_val) = value.parse::<f64>() {
        // Create number from float, handling special case where float is actually an
        // integer
        return if float_val.fract() == 0.0 {
            Value::Number(serde_json::Number::from(float_val as i64))
        } else if let Some(num) = serde_json::Number::from_f64(float_val) {
            Value::Number(num)
        } else {
            Value::String(value.to_string())
        };
    }

    // Default to string if no other type matches
    Value::String(value.to_string())
}

impl From<Value> for ToolCallArguments {
    fn from(value: Value) -> Self {
        ToolCallArguments::Parsed(value)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_serialize_unparsed_valid_json() {
        let fixture = ToolCallArguments::from_json(r#"{"param": "value", "count": 42}"#);
        let actual = serde_json::to_string(&fixture).unwrap();
        // The RawValue preserves the original JSON string when it's valid
        let expected = r#"{"param": "value", "count": 42}"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_unparsed_valid_json_array() {
        let fixture = ToolCallArguments::from_json(r#"["item1", "item2", 123]"#);
        let actual = serde_json::to_string(&fixture).unwrap();
        // The RawValue preserves the original JSON string when it's valid
        let expected = r#"["item1", "item2", 123]"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_unparsed_valid_json_nested() {
        let fixture = ToolCallArguments::from_json(
            r#"{"user": {"name": "John", "settings": {"theme": "dark"}}}"#,
        );
        let actual = serde_json::to_string(&fixture).unwrap();
        // The RawValue preserves the original JSON string when it's valid
        let expected = r#"{"user": {"name": "John", "settings": {"theme": "dark"}}}"#;
        assert_eq!(actual, expected);
    }
    #[test]
    fn test_serialize_unparsed_valid_json_compact() {
        let fixture = ToolCallArguments::from_json(r#"{"param":"value","count":42}"#);
        let actual = serde_json::to_string(&fixture).unwrap();
        // The RawValue preserves the original JSON string when it's valid
        let expected = r#"{"param":"value","count":42}"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_unparsed_invalid_json() {
        let fixture = ToolCallArguments::from_json(r#"{"param": "value", invalid}"#);
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = r#""{\"param\": \"value\", invalid}""#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_unparsed_malformed_json() {
        let fixture = ToolCallArguments::from_json("not json at all");
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = r#""not json at all""#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_unparsed_empty_string() {
        let fixture = ToolCallArguments::from_json("");
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = r#""""#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_parsed_object() {
        let fixture = ToolCallArguments::Parsed(json!({
            "name": "test",
            "value": 42,
            "enabled": true
        }));
        let actual = serde_json::to_string(&fixture).unwrap();
        // The order of keys in JSON object serialization is not guaranteed unless using indexmap, 
        // so we should parse back and compare the objects.
        let actual_json: serde_json::Value = serde_json::from_str(&actual).unwrap();
        let expected_json = json!({
            "name": "test",
            "value": 42,
            "enabled": true
        });
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn test_serialize_parsed_array() {
        let fixture = ToolCallArguments::Parsed(json!(["a", "b", 123, true, null]));
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = r#"["a","b",123,true,null]"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_parsed_primitive_string() {
        let fixture = ToolCallArguments::Parsed(json!("simple string"));
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = r#""simple string""#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_parsed_primitive_number() {
        let fixture = ToolCallArguments::Parsed(json!(42));
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = "42";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_parsed_primitive_boolean() {
        let fixture = ToolCallArguments::Parsed(json!(true));
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = "true";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_parsed_null() {
        let fixture = ToolCallArguments::Parsed(json!(null));
        let actual = serde_json::to_string(&fixture).unwrap();
        let expected = "null";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_deserialize_valid_json_object() {
        let json_str = r#"{"param": "value", "count": 42}"#;
        let actual: ToolCallArguments = serde_json::from_str(json_str).unwrap();
        let expected = ToolCallArguments::Parsed(json!({
            "param": "value",
            "count": 42
        }));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_deserialize_valid_json_array() {
        let json_str = r#"["item1", "item2", 123]"#;
        let actual: ToolCallArguments = serde_json::from_str(json_str).unwrap();
        let expected = ToolCallArguments::Parsed(json!(["item1", "item2", 123]));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_deserialize_primitive_string() {
        let json_str = r#""simple string""#;
        let actual: ToolCallArguments = serde_json::from_str(json_str).unwrap();
        let expected = ToolCallArguments::Parsed(json!("simple string"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_roundtrip_unparsed_valid_json() {
        let original_json = r#"{"param": "value", "count": 42}"#;
        let fixture = ToolCallArguments::from_json(original_json);
        let serialized = serde_json::to_string(&fixture).unwrap();
        let deserialized: ToolCallArguments = serde_json::from_str(&serialized).unwrap();
        let expected = ToolCallArguments::Parsed(json!({
            "param": "value",
            "count": 42
        }));
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_roundtrip_parsed_value() {
        let fixture = ToolCallArguments::Parsed(json!({
            "name": "test",
            "value": 42,
            "enabled": true
        }));
        let serialized = serde_json::to_string(&fixture).unwrap();
        let actual: ToolCallArguments = serde_json::from_str(&serialized).unwrap();
        let expected = fixture;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_unparsed_valid_json() {
        let fixture = ToolCallArguments::from_json(r#"{"param": "value"}"#);
        let actual = fixture.parse().unwrap();
        let expected = json!({"param": "value"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_unparsed_invalid_json_with_repair() {
        let fixture = ToolCallArguments::from_json(r#"{"param": "value", "missing_quote": true"#);
        let actual = fixture.parse().unwrap();
        let expected = json!({"param": "value", "missing_quote": true});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_parsed_value() {
        let value = json!({"param": "value"});
        let fixture = ToolCallArguments::Parsed(value.clone());
        let actual = fixture.parse().unwrap();
        let expected = value;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_parameters() {
        let mut params = BTreeMap::new();
        params.insert("name".to_string(), "John".to_string());
        params.insert("age".to_string(), "30".to_string());
        params.insert("active".to_string(), "true".to_string());
        params.insert("score".to_string(), "95.5".to_string());

        let actual = ToolCallArguments::from_parameters(params);
        let expected = ToolCallArguments::Parsed(json!({
            "name": "John",
            "age": 30,
            "active": true,
            "score": 95.5
        }));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_into_string_unparsed() {
        let fixture = ToolCallArguments::from_json(r#"{"param": "value"}"#);
        let actual = fixture.into_string();
        let expected = r#"{"param": "value"}"#;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_into_string_parsed() {
        let fixture = ToolCallArguments::Parsed(json!({"param": "value"}));
        let actual = fixture.into_string();
        let expected = r#"{"param":"value"}"#;
        assert_eq!(actual, expected);
    }
}
