use schemars::schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec};
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
    coerce_value_with_schema(value, &Schema::Object(schema.schema.clone()))
}

fn coerce_value_with_schema(value: Value, schema: &Schema) -> Value {
    match schema {
        Schema::Object(schema_obj) => coerce_value_with_schema_object(value, schema_obj),
        Schema::Bool(_) => value, // Boolean schemas don't provide type info for coercion
    }
}

fn coerce_value_with_schema_object(value: Value, schema: &SchemaObject) -> Value {
    // Handle anyOf/oneOf schemas by trying each sub-schema
    if let Some(subschemas) = &schema.subschemas {
        if let Some(any_of) = &subschemas.any_of {
            // Try each sub-schema in anyOf until one succeeds
            for sub_schema in any_of {
                let result = coerce_value_with_schema(value.clone(), sub_schema);
                if result != value {
                    return result;
                }
            }
        }
        if let Some(one_of) = &subschemas.one_of {
            // Try each sub-schema in oneOf until one succeeds
            for sub_schema in one_of {
                let result = coerce_value_with_schema(value.clone(), sub_schema);
                if result != value {
                    return result;
                }
            }
        }
        if let Some(all_of) = &subschemas.all_of {
            // Apply all schemas in sequence
            let mut result = value;
            for sub_schema in all_of {
                result = coerce_value_with_schema(result, sub_schema);
            }
            return result;
        }
    }

    // Handle $ref schemas by resolving references
    if let Some(_reference) = &schema.reference {
        // For now, we'll fall back to the original schema logic if we can't
        // resolve In a more complete implementation, we'd resolve the
        // reference against the root schema But for this fix, we'll
        // preserve the current behavior for $ref cases
    }

    // Handle objects with properties
    if let Value::Object(mut map) = value {
        if let Some(object_validation) = &schema.object {
            for (key, val) in map.iter_mut() {
                if let Some(prop_schema) = object_validation.properties.get(key) {
                    let coerced = coerce_value_with_schema(val.clone(), prop_schema);
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
                            .map(|item| coerce_value_with_schema(item, item_schema))
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
                                    .map(|schema| coerce_value_with_schema(item.clone(), schema))
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
        InstanceType::String | InstanceType::Object | InstanceType::Array => {
            // Don't coerce to these types from strings
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use schemars::schema::{
        InstanceType, ObjectValidation, RootSchema, Schema, SchemaObject, SingleOrVec,
        SubschemaValidation,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn test_coerce_string_to_integer() {
        let fixture = json!({"age": "42"});
        let schema = integer_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"age": 42});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_multiple_string_integers() {
        let fixture = json!({"start": "100", "end": "200"});
        let schema = two_integer_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"start": 100, "end": 200});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_number_float() {
        let fixture = json!({"price": "19.99"});
        let schema = number_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"price": 19.99});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_boolean() {
        let fixture = json!({"active": "true", "disabled": "false"});
        let schema = boolean_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"active": true, "disabled": false});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_coercion_when_types_match() {
        let fixture = json!({"age": 42});
        let schema = integer_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"age": 42});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_coercion_for_invalid_strings() {
        let fixture = json!({"age": "not_a_number"});
        let schema = integer_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"age": "not_a_number"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_nested_objects() {
        let fixture = json!({"user": {"age": "30", "score": "95.5"}});
        let schema = nested_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"user": {"age": 30, "score": 95.5}});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_array_items() {
        let fixture = json!({"numbers": ["1", "2", "3"]});
        let schema = array_schema();
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"numbers": [1, 2, 3]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserve_non_string_values() {
        let fixture = json!({"name": "John", "age": 42, "active": true});
        let schema = mixed_schema();
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

        // Schema matching FSRead structure
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "path".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
                ..Default::default()
            }),
        );
        properties.insert(
            "start_line".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );
        properties.insert(
            "end_line".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

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
        // Test schema with anyOf that allows integers or null
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    any_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Null))),
                            ..Default::default()
                        }),
                    ]),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing string to integer
        let fixture = json!({"value": "42"});
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
        // Test schema with oneOf that allows only integers or booleans
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    one_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Boolean,
                            ))),
                            ..Default::default()
                        }),
                    ]),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing string to integer
        let fixture = json!({"value": "123"});
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
        // Test schema with allOf composition
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    all_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Number,
                            ))),
                            ..Default::default()
                        }),
                    ]),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing string to integer via allOf composition
        let fixture = json!({"value": "42"});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 42});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_any_of_preserves_original_when_no_match() {
        // Test that anyOf preserves original value when no subschema matches
        // Note: oneOf behaves similarly
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    any_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Boolean,
                            ))),
                            ..Default::default()
                        }),
                    ]),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test preserving invalid string
        let fixture = json!({"value": "not_a_number"});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": "not_a_number"});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_any_of_with_number_coercion() {
        // Test anyOf with number coercion
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    any_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Number,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Null))),
                            ..Default::default()
                        }),
                    ]),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing string to float
        let fixture = json!({"value": "2.14"});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 2.14});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_array_with_tuple_schema() {
        // Test array with tuple schema (SingleOrVec::Vec)
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "coordinates".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Array))),
                array: Some(Box::new(schemars::schema::ArrayValidation {
                    items: Some(SingleOrVec::Vec(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Number,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Number,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Number,
                            ))),
                            ..Default::default()
                        }),
                    ])),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing string numbers in tuple array
        let fixture = json!({"coordinates": ["1.5", "2.5", "3.5"]});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"coordinates": [1.5, 2.5, 3.5]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_array_with_tuple_schema_mixed_types() {
        // Test array with tuple schema with mixed types
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "data".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Array))),
                array: Some(Box::new(schemars::schema::ArrayValidation {
                    items: Some(SingleOrVec::Vec(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::String,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Boolean,
                            ))),
                            ..Default::default()
                        }),
                    ])),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing mixed types in tuple array
        let fixture = json!({"data": ["name", "42", "true"]});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"data": ["name", 42, true]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_array_with_tuple_schema_extra_items() {
        // Test that extra items in tuple array are preserved
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "items".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Array))),
                array: Some(Box::new(schemars::schema::ArrayValidation {
                    items: Some(SingleOrVec::Vec(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                    ])),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test that extra items are preserved
        let fixture = json!({"items": ["1", "2", "3", "4"]});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"items": [1, 2, "3", "4"]});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_nested_any_of_in_object() {
        // Test anyOf nested in object properties
        let mut nested_properties = std::collections::BTreeMap::new();
        nested_properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    any_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        }),
                        Schema::Object(SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Boolean,
                            ))),
                            ..Default::default()
                        }),
                    ]),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "nested".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties: nested_properties,
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing in nested object with anyOf
        let fixture = json!({"nested": {"value": "42"}});
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"nested": {"value": 42}});
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_coerce_string_to_null() {
        // Test coercing string "null" to null type
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Null))),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing "null" string to null
        let fixture = json!({"value": "null"});
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
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Boolean))),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

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
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

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
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "value".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );

        let schema = RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        // Test coercing large unsigned integer that doesn't fit in i64
        let fixture = json!({"value": "18446744073709551615"}); // u64::MAX
        let actual = coerce_to_schema(fixture, &schema);
        let expected = json!({"value": 18446744073709551615u64});
        assert_eq!(actual, expected);
    }

    // Helper functions to create test schemas
    fn integer_schema() -> RootSchema {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "age".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn two_integer_schema() -> RootSchema {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "start".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );
        properties.insert(
            "end".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn number_schema() -> RootSchema {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "price".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Number))),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn boolean_schema() -> RootSchema {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "active".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Boolean))),
                ..Default::default()
            }),
        );
        properties.insert(
            "disabled".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Boolean))),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn nested_schema() -> RootSchema {
        let mut user_properties = std::collections::BTreeMap::new();
        user_properties.insert(
            "age".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );
        user_properties.insert(
            "score".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Number))),
                ..Default::default()
            }),
        );

        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "user".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties: user_properties,
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn array_schema() -> RootSchema {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "numbers".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Array))),
                array: Some(Box::new(schemars::schema::ArrayValidation {
                    items: Some(SingleOrVec::Single(Box::new(Schema::Object(
                        SchemaObject {
                            instance_type: Some(SingleOrVec::Single(Box::new(
                                InstanceType::Integer,
                            ))),
                            ..Default::default()
                        },
                    )))),
                    ..Default::default()
                })),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn mixed_schema() -> RootSchema {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert(
            "name".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
                ..Default::default()
            }),
        );
        properties.insert(
            "age".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                ..Default::default()
            }),
        );
        properties.insert(
            "active".to_string(),
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Boolean))),
                ..Default::default()
            }),
        );

        RootSchema {
            schema: SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
