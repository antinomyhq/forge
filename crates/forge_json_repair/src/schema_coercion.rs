use schemars::schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec};
use serde::de::Error as _;
use serde_json::Value;

/// Coerces a JSON value to match the expected types defined in a JSON schema.
///
/// This function recursively traverses the JSON value and the schema,
/// converting string values to the expected types (e.g., "42" -> 42) when the
/// schema indicates a different type is expected.
///
/// # Arguments
///
/// * `value` - The JSON value to coerce
/// * `schema` - The JSON schema defining expected types
///
/// # Errors
///
/// Returns the original value if coercion is not possible or the schema doesn't
/// specify type constraints.
pub fn coerce_to_schema(value: Value, schema: &RootSchema) -> Value {
    coerce_value_with_schema(value, &Schema::Object(schema.schema.clone()), schema)
}

fn coerce_value_with_schema(value: Value, schema: &Schema, root_schema: &RootSchema) -> Value {
    match schema {
        Schema::Object(schema_obj) => {
            coerce_value_with_schema_object(value, schema_obj, root_schema)
        }
        Schema::Bool(_) => value, // Boolean schemas don't provide type info for coercion
    }
}

fn coerce_value_with_schema_object(
    value: Value,
    schema: &SchemaObject,
    root_schema: &RootSchema,
) -> Value {
    // Handle $ref schemas by resolving references
    if let Some(reference) = &schema.reference {
        // Resolve $ref against root schema definitions
        // schemars uses format: "#/definitions/TypeName"
        if let Some(def_name) = reference.strip_prefix("#/definitions/")
            && let Some(def_schema) = root_schema.definitions.get(def_name)
        {
            return coerce_value_with_schema(value, def_schema, root_schema);
        }
    }
    // Handle anyOf/oneOf schemas by trying each sub-schema
    if let Some(subschemas) = &schema.subschemas {
        if let Some(any_of) = &subschemas.any_of {
            // Try each sub-schema in anyOf until one succeeds
            for sub_schema in any_of {
                let result = coerce_value_with_schema(value.clone(), sub_schema, root_schema);
                if result != value {
                    return result;
                }
            }
        }
        if let Some(one_of) = &subschemas.one_of {
            // Try each sub-schema in oneOf until one succeeds
            for sub_schema in one_of {
                let result = coerce_value_with_schema(value.clone(), sub_schema, root_schema);
                if result != value {
                    return result;
                }
            }
        }
        if let Some(all_of) = &subschemas.all_of {
            // Apply all schemas in sequence
            let mut result = value;
            for sub_schema in all_of {
                result = coerce_value_with_schema(result, sub_schema, root_schema);
            }
            return result;
        }
    }

    // Handle objects with properties
    if let Value::Object(mut map) = value {
        if let Some(object_validation) = &schema.object {
            for (key, val) in map.iter_mut() {
                if let Some(prop_schema) = object_validation.properties.get(key) {
                    let coerced = coerce_value_with_schema(val.clone(), prop_schema, root_schema);
                    *val = coerced;
                }
            }
        }
        return Value::Object(map);
    }

    // Handle arrays
    if let Value::Array(arr) = value {
        if let Some(array_validation) = &schema.array
            && let Some(items_schema) = &array_validation.items
        {
            match items_schema {
                SingleOrVec::Single(item_schema) => {
                    return Value::Array(
                        arr.into_iter()
                            .map(|item| coerce_value_with_schema(item, item_schema, root_schema))
                            .collect(),
                    );
                }
                SingleOrVec::Vec(item_schemas) => {
                    return Value::Array(
                        arr.into_iter()
                            .enumerate()
                            .map(|(i, item)| {
                                item_schemas
                                    .get(i)
                                    .map(|schema| {
                                        coerce_value_with_schema(item.clone(), schema, root_schema)
                                    })
                                    .unwrap_or(item)
                            })
                            .collect(),
                    );
                }
            }
        }
        return Value::Array(arr);
    }

    // If schema has specific instance types, try to coerce the value
    if let Some(instance_types) = &schema.instance_type {
        return coerce_by_instance_type(value, instance_types);
    }

    value
}

fn coerce_by_instance_type(value: Value, instance_types: &SingleOrVec<InstanceType>) -> Value {
    let target_types: Vec<&InstanceType> = match instance_types {
        SingleOrVec::Single(t) => vec![t.as_ref()],
        SingleOrVec::Vec(types) => types.iter().collect(),
    };

    // If the value already matches one of the target types, return as-is
    if type_matches(&value, &target_types) {
        return value;
    }

    // Try coercion if value is a string
    if let Value::String(s) = &value {
        for target_type in target_types {
            if let Some(coerced) = try_coerce_string(s, target_type) {
                return coerced;
            }
        }
    }

    value
}

fn type_matches(value: &Value, target_types: &[&InstanceType]) -> bool {
    target_types.iter().any(|t| match t {
        InstanceType::Null => value.is_null(),
        InstanceType::Boolean => value.is_boolean(),
        InstanceType::Object => value.is_object(),
        InstanceType::Array => value.is_array(),
        InstanceType::Number => value.is_number(),
        InstanceType::String => value.is_string(),
        InstanceType::Integer => value.is_i64() || value.is_u64(),
    })
}

fn try_coerce_string(s: &str, target_type: &InstanceType) -> Option<Value> {
    match target_type {
        InstanceType::Integer => {
            // Try to parse as i64
            if let Ok(num) = s.parse::<i64>() {
                return Some(Value::Number(num.into()));
            }
            // Try to parse as u64
            if let Ok(num) = s.parse::<u64>() {
                return Some(Value::Number(num.into()));
            }
            None
        }
        InstanceType::Number => {
            // Try to parse as integer first
            if let Ok(num) = s.parse::<i64>() {
                return Some(Value::Number(num.into()));
            }
            // Then try float
            if let Ok(num) = s.parse::<f64>()
                && let Some(json_num) = serde_json::Number::from_f64(num)
            {
                return Some(Value::Number(json_num));
            }
            None
        }
        InstanceType::Boolean => match s.trim().to_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        InstanceType::Null => {
            if s.trim().to_lowercase() == "null" {
                Some(Value::Null)
            } else {
                None
            }
        }
        InstanceType::String => {
            // Keep as string
            None
        }
        InstanceType::Object => {
            // Try to parse the string as a JSON object
            if let Ok(parsed) = try_parse_json_string(s)
                && parsed.is_object()
            {
                return Some(parsed);
            }
            None
        }
        InstanceType::Array => {
            // Try to parse the string as a JSON array
            if let Ok(parsed) = try_parse_json_string(s)
                && parsed.is_array()
            {
                return Some(parsed);
            }
            None
        }
    }
}

/// Attempts to parse a string as JSON, handling both valid JSON and JSON5
/// (Python-style) syntax
fn try_parse_json_string(s: &str) -> Result<Value, serde_json::Error> {
    // First try parsing as-is (valid JSON)
    if let Ok(parsed) = serde_json::from_str::<Value>(s) {
        return Ok(parsed);
    }

    // If that fails, try parsing as JSON5 (handles single quotes, comments, etc.)
    // Convert serde_json5::Error to serde_json::Error
    serde_json5::from_str::<Value>(s).map_err(|e| serde_json::Error::custom(e.to_string()))
}

#[cfg(test)]
mod tests {
    #![allow(dead_code)]
    use schemars::{JsonSchema, schema_for};
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::*;

    // Test structs with JsonSchema derive
    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct AgeData {
        age: i64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct RangeData {
        start: i64,
        end: i64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct PriceData {
        price: f64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct BooleanData {
        active: bool,
        disabled: bool,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct UserData {
        age: i64,
        score: f64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct UserWrapper {
        user: UserData,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct NumbersData {
        numbers: Vec<i64>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct MixedData {
        name: String,
        age: i64,
        active: bool,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct PathData {
        path: String,
        start_line: i64,
        end_line: i64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct IntOrNull {
        value: Option<i64>,
    }

    #[derive(JsonSchema, Deserialize, Serialize)]
    #[allow(dead_code)]
    #[serde(untagged)]
    enum IntOrBool {
        Int(i64),
        Bool(bool),
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct IntOrBoolData {
        value: IntOrBool,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct AllOfIntNumber {
        value: i64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct CoordinatesData {
        coordinates: [f64; 3],
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct MixedTupleData {
        data: (String, i64, bool),
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct TupleItems {
        items: [i64; 2],
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct ExtraItemsData {
        items: Vec<serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct NestedUnionData {
        nested: IntOrNull,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct NullData {
        value: Option<()>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct BoolData {
        value: bool,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct LargeIntData {
        value: i64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct UnsignedIntData {
        value: u64,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct ArrayData {
        items: Vec<i64>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct EditsData {
        edits: Vec<serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct ConfigData {
        config: std::collections::BTreeMap<String, serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct DataArray {
        data: Vec<serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct ItemsArray {
        items: Vec<serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct ConfigWithComments {
        config: std::collections::BTreeMap<String, serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct ItemsTrailingComma {
        items: Vec<serde_json::Value>,
    }

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct MultiPatchData {
        edits: Vec<serde_json::Value>,
    }

    #[test]
    fn test_coerce_string_to_integer() {
        let fixture = json!({"age": "42"});
        let schema = schema_for!(AgeData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"age": 42});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_multiple_string_integers() {
        let fixture = json!({"start": "100", "end": "200"});
        let schema = schema_for!(RangeData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"start": 100, "end": 200});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_number_float() {
        let fixture = json!({"price": "19.99"});
        let schema = schema_for!(PriceData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"price": 19.99});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_boolean() {
        let fixture = json!({"active": "true", "disabled": "false"});
        let schema = schema_for!(BooleanData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"active": true, "disabled": false});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_coercion_when_types_match() {
        let fixture = json!({"age": 42});
        let schema = schema_for!(AgeData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"age": 42});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_coercion_for_invalid_strings() {
        let fixture = json!({"age": "not_a_number"});
        let schema = schema_for!(AgeData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"age": "not_a_number"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_nested_objects() {
        let fixture = json!({"user": {"age": "30", "score": "95.5"}});
        let schema = schema_for!(UserWrapper);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"user": {"age": 30, "score": 95.5}});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_array_items() {
        let fixture = json!({"numbers": ["1", "2", "3"]});
        let schema = schema_for!(NumbersData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"numbers": [1, 2, 3]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserve_non_string_values() {
        let fixture = json!({"name": "John", "age": 42, "active": true});
        let schema = schema_for!(MixedData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"name": "John", "age": 42, "active": true});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_read_tool_line_numbers() {
        // Simulate the exact case from the task: read tool with string line numbers
        let fixture = json!({
            "path": "/Users/amit/code-forge/crates/forge_main/src/ui.rs",
            "start_line": "2255",
            "end_line": "2285"
        });

        let schema = schema_for!(PathData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({
            "path": "/Users/amit/code-forge/crates/forge_main/src/ui.rs",
            "start_line": 2255,
            "end_line": 2285
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_any_of_union_types() {
        // Test coercing string to integer
        let fixture = json!({"value": "42"});
        let schema = schema_for!(IntOrNull);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 42});
        assert_eq!(actual, expected);

        // Test preserving null
        let fixture = json!({"value": null});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": null});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_one_of_union_types() {
        // Test coercing string to integer
        let fixture = json!({"value": "123"});
        let schema = schema_for!(IntOrBoolData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 123});
        assert_eq!(actual, expected);

        // Test coercing string to boolean
        let fixture = json!({"value": "true"});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": true});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_all_of_composition() {
        // Test coercing string to integer via allOf composition
        let fixture = json!({"value": "42"});
        let schema = schema_for!(AllOfIntNumber);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 42});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_any_of_preserves_original_when_no_match() {
        // Test that anyOf preserves original value when no subschema matches
        // Note: oneOf behaves similarly
        let fixture = json!({"value": "not_a_number"});
        let schema = schema_for!(IntOrBoolData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": "not_a_number"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_any_of_with_number_coercion() {
        // Test anyOf with number coercion
        let fixture = json!({"value": "2.14"});
        let schema = schema_for!(IntOrNull);
        let actual = coerce_to_schema(fixture, &schema);
        // The anyOf schema tries each subschema; since "2.14" can't be parsed as i64,
        // it returns the original value
        let expected = json!({"value": "2.14"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_array_with_tuple_schema() {
        // Test array with tuple schema (SingleOrVec::Vec)
        let fixture = json!({"coordinates": ["1.5", "2.5", "3.5"]});
        let schema = schema_for!(CoordinatesData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"coordinates": [1.5, 2.5, 3.5]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_array_with_tuple_schema_mixed_types() {
        // Test array with tuple schema with mixed types
        let fixture = json!({"data": ["name", "42", "true"]});
        let schema = schema_for!(MixedTupleData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"data": ["name", 42, true]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_array_with_tuple_schema_extra_items() {
        // Test that Vec<serde_json::Value> doesn't coerce items (no type constraints)
        let fixture = json!({"items": ["1", "2", "3", "4"]});
        let schema = schema_for!(ExtraItemsData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"items": ["1", "2", "3", "4"]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_nested_any_of_in_object() {
        // Test coercing in nested object with anyOf
        let fixture = json!({"nested": {"value": "42"}});
        let schema = schema_for!(NestedUnionData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"nested": {"value": 42}});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_null() {
        // Test coercing "null" string to null
        let fixture = json!({"value": "null"});
        let schema = schema_for!(NullData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": null});
        assert_eq!(actual, expected);

        // Test that "NULL" (uppercase) also works
        let fixture = json!({"value": "NULL"});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": null});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_boolean_case_insensitive() {
        // Test that boolean coercion is case-insensitive
        let schema = schema_for!(BoolData);

        // Test various case variations
        for (input, expected) in [
            ("true", true),
            ("TRUE", true),
            ("True", true),
            ("false", false),
            ("FALSE", false),
            ("False", false),
        ] {
            let fixture = json!({"value": input});
            let actual = coerce_to_schema(fixture, &schema);
            let expected = json!({"value": expected});
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_coerce_large_integer() {
        // Test coercing large integers that fit in i64
        let schema = schema_for!(LargeIntData);

        // Test coercing large positive integer
        let fixture = json!({"value": "9223372036854775807"}); // i64::MAX
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 9223372036854775807i64});
        assert_eq!(actual, expected);

        // Test coercing large negative integer
        let fixture = json!({"value": "-9223372036854775808"}); // i64::MIN
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": -9223372036854775808i64});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_unsigned_integer() {
        // Test coercing unsigned integers (u64)
        let schema = schema_for!(UnsignedIntData);

        // Test coercing large unsigned integer that doesn't fit in i64
        let fixture = json!({"value": "18446744073709551615"}); // u64::MAX
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 18446744073709551615u64});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_array() {
        // Test coercing a JSON array string to an actual array
        let fixture = json!({"items": "[1, 2, 3]"});
        let schema = schema_for!(ArrayData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"items": [1, 2, 3]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_python_style_string_to_array() {
        // Test coercing a Python-style array string to an actual array
        let fixture = json!({"edits": "[{'content': 'test', 'operation': 'replace'}]"});
        let schema = schema_for!(EditsData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"edits": [{"content": "test", "operation": "replace"}]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_python_style_string_to_object() {
        // Test coercing a Python-style object string to an actual object
        let fixture = json!({"config": "{'key': 'value', 'number': 42}"});
        let schema = schema_for!(ConfigData);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"config": {"key": "value", "number": 42}});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserve_invalid_json_string() {
        // Test that invalid JSON strings are preserved
        let fixture = json!({"data": "[invalid json"});
        let schema = schema_for!(DataArray);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"data": "[invalid json"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_json5_with_comments() {
        // Test coercing JSON5 with comments
        let fixture = json!({"config": r#"{
            // This is a comment
            "key": "value",
            "number": 42,
        }"#});
        let schema = schema_for!(ConfigWithComments);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"config": {"key": "value", "number": 42}});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_json5_with_trailing_commas() {
        // Test coercing JSON5 with trailing commas
        let fixture = json!({"items": "[1, 2, 3,]"});
        let schema = schema_for!(ItemsTrailingComma);
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"items": [1, 2, 3]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_multi_patch_python_style() {
        // Test coercing exact Python-style input from error
        // This matches multi_patch tool call format with nested objects
        let python_style = r#"[{'content': 'use schemars::schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec};', 'operation': 'replace', 'path': 'crates/forge_json_repair/src/schema_coercion.rs'}, {'content': 'fn coerce_value_with_schema(value: Value, schema: &Schema) -> Value {', 'operation': 'replace', 'path': 'crates/forge_json_repair/src/schema_coercion.rs'}]"#;

        let fixture = json!({"edits": python_style});
        let schema = schema_for!(MultiPatchData);
        let actual = coerce_to_schema(fixture, &schema);

        // Should coerce string to an array of objects
        assert!(actual["edits"].is_array());
        let edits = actual["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 2);

        // Verify first edit object
        assert_eq!(
            edits[0]["content"],
            "use schemars::schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec};"
        );
        assert_eq!(edits[0]["operation"], "replace");
        assert_eq!(
            edits[0]["path"],
            "crates/forge_json_repair/src/schema_coercion.rs"
        );

        // Verify second edit object
        assert_eq!(
            edits[1]["content"],
            "fn coerce_value_with_schema(value: Value, schema: &Schema) -> Value {"
        );
        assert_eq!(edits[1]["operation"], "replace");
        assert_eq!(
            edits[1]["path"],
            "crates/forge_json_repair/src/schema_coercion.rs"
        );
    }
}
