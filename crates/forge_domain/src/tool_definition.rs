use derive_setters::Setters;
use schemars::generate::SchemaSettings;
use schemars::transform::{
    AddNullable, RemoveRefSiblings, ReplaceUnevaluatedProperties, RestrictFormats, Transform,
};
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{NamedTool, ToolCallContext, ToolName, ToolOutput};

///
/// Refer to the specification over here:
/// https://glama.ai/blog/2024-11-25-model-context-protocol-quickstart#server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub description: String,
    pub input_schema: Schema,
    pub output_schema: Option<Schema>,
}

impl ToolDefinition {
    /// Create a new ToolDefinition
    pub fn new<N: ToString>(name: N) -> Self {
        ToolDefinition {
            name: ToolName::new(name),
            description: String::new(),
            input_schema: schemars::schema_for!(()), // Empty input schema
            output_schema: None,
        }
    }
}

/// Transform that flattens schemas by removing $schema field and inlining
/// definitions
#[derive(Clone)]
struct FlattenSchema;

impl Transform for FlattenSchema {
    fn transform(&mut self, schema: &mut schemars::Schema) {
        // Convert to JSON value to manipulate
        let mut schema_value = serde_json::to_value(&schema).unwrap();

        // Apply our transformations
        if let Some(obj) = schema_value.as_object_mut() {
            // Remove $schema field
            obj.remove("$schema");

            // Handle definitions
            if let Some(definitions) = obj.get("definitions").cloned() {
                obj.remove("definitions");

                // Inline all references
                inline_json_refs(&mut schema_value, &definitions);
            }
        }

        // Simplify enums
        simplify_json_enums(&mut schema_value);

        // Convert back to Schema
        *schema = serde_json::from_value(schema_value).unwrap();
    }
}

fn inline_json_refs(value: &mut serde_json::Value, definitions: &serde_json::Value) {
    match value {
        serde_json::Value::Object(obj) => {
            // Check if this is a direct $ref
            if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
                if let Some(def_name) = ref_str.strip_prefix("#/definitions/") {
                    if let Some(def_value) = definitions.get(def_name) {
                        // Keep any other properties that might exist
                        let mut other_props = serde_json::Map::new();
                        for (k, v) in obj.iter() {
                            if k != "$ref" {
                                other_props.insert(k.clone(), v.clone());
                            }
                        }

                        // Replace with the definition
                        *value = def_value.clone();

                        // Merge back any other properties
                        if let Some(new_obj) = value.as_object_mut() {
                            for (k, v) in other_props {
                                new_obj.insert(k, v);
                            }
                        }

                        // Continue processing the inlined value
                        inline_json_refs(value, definitions);
                        return;
                    }
                }
            }

            // Check if this is an allOf with a single $ref
            if let Some(all_of) = obj.get("allOf") {
                if let Some(arr) = all_of.as_array() {
                    if arr.len() == 1 {
                        if let Some(ref_obj) = arr[0].as_object() {
                            if let Some(ref_str) = ref_obj.get("$ref").and_then(|v| v.as_str()) {
                                if let Some(def_name) = ref_str.strip_prefix("#/definitions/") {
                                    if let Some(def_value) = definitions.get(def_name) {
                                        // Keep the description if it exists
                                        let description = obj.get("description").cloned();
                                        *value = def_value.clone();

                                        // Restore the description
                                        if let (Some(desc), Some(new_obj)) =
                                            (description, value.as_object_mut())
                                        {
                                            new_obj.insert("description".to_string(), desc);
                                        }

                                        // Continue processing
                                        inline_json_refs(value, definitions);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Clone keys to avoid borrow issues
            let keys: Vec<String> = obj.keys().cloned().collect();
            for key in keys {
                if let Some(v) = obj.get_mut(&key) {
                    inline_json_refs(v, definitions);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                inline_json_refs(v, definitions);
            }
        }
        _ => {}
    }
}

fn simplify_json_enums(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(obj) => {
            // Check if this is a oneOf that can be simplified
            if let Some(one_of) = obj.get("oneOf") {
                if let Some(arr) = one_of.as_array() {
                    let mut enum_values = Vec::new();
                    let mut all_are_const_strings = true;

                    // Check if all items are const strings
                    for item in arr {
                        if let Some(item_obj) = item.as_object() {
                            if let Some(const_val) = item_obj.get("const") {
                                if let Some(s) = const_val.as_str() {
                                    enum_values.push(serde_json::Value::String(s.to_string()));
                                } else {
                                    all_are_const_strings = false;
                                    break;
                                }
                            } else {
                                all_are_const_strings = false;
                                break;
                            }
                        } else {
                            all_are_const_strings = false;
                            break;
                        }
                    }

                    // If all are const strings, replace with enum
                    if all_are_const_strings && !enum_values.is_empty() {
                        obj.remove("oneOf");
                        obj.insert(
                            "type".to_string(),
                            serde_json::Value::String("string".to_string()),
                        );
                        obj.insert("enum".to_string(), serde_json::Value::Array(enum_values));
                    }
                }
            }

            // Clone keys to avoid borrow issues
            let keys: Vec<String> = obj.keys().cloned().collect();
            for key in keys {
                if let Some(v) = obj.get_mut(&key) {
                    simplify_json_enums(v);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                simplify_json_enums(v);
            }
        }
        _ => {}
    }
}
impl<T> From<&T> for ToolDefinition
where
    T: NamedTool + ExecutableTool + ToolDescription + Send + Sync + 'static,
    T::Input: serde::de::DeserializeOwned + JsonSchema,
{
    fn from(t: &T) -> Self {
        let setting = SchemaSettings::draft07();
        let generator = setting
            .with_transform(AddNullable::default())
            .with_transform(RemoveRefSiblings::default())
            .with_transform(ReplaceUnevaluatedProperties::default())
            .with_transform(RestrictFormats::default())
            .with_transform(FlattenSchema)
            .into_generator();
        let input: Schema = generator.into_root_schema_for::<T::Input>();

        ToolDefinition {
            name: T::tool_name(),
            description: t.description(),
            input_schema: input,
            output_schema: None,
        }
    }
}

pub trait ToolDescription {
    fn description(&self) -> String;
}

#[async_trait::async_trait]
pub trait ExecutableTool {
    type Input: DeserializeOwned;

    async fn call(
        &self,
        context: ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput>;
}

#[cfg(test)]
mod tests {
    use schemars::generate::SchemaSettings;
    use schemars::transform::{
        AddNullable, RemoveRefSiblings, ReplaceUnevaluatedProperties, RestrictFormats,
    };
    use schemars::JsonSchema;
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct TestInput {
        path: String,
        search: String,
        operation: PatchOperation,
        content: String,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    enum PatchOperation {
        #[serde(rename = "prepend")]
        Prepend,
        #[serde(rename = "append")]
        Append,
        #[serde(rename = "replace")]
        Replace,
        #[serde(rename = "swap")]
        Swap,
    }

    struct TestTool;

    impl NamedTool for TestTool {
        fn tool_name() -> ToolName {
            ToolName::new("test_tool")
        }
    }

    impl ToolDescription for TestTool {
        fn description(&self) -> String {
            "Test tool".to_string()
        }
    }

    #[async_trait::async_trait]
    impl ExecutableTool for TestTool {
        type Input = TestInput;

        async fn call(
            &self,
            _context: ToolCallContext,
            _input: Self::Input,
        ) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text("test".to_string()))
        }
    }

    #[test]
    fn test_schema_flattening() {
        let tool = TestTool;

        // First generate without our transform to see the original
        let setting = SchemaSettings::draft07();
        let generator = setting
            .with_transform(AddNullable::default())
            .with_transform(RemoveRefSiblings::default())
            .with_transform(ReplaceUnevaluatedProperties::default())
            .with_transform(RestrictFormats::default())
            .into_generator();
        let original_schema = generator.into_root_schema_for::<TestInput>();
        let original_json = serde_json::to_value(&original_schema).unwrap();

        println!(
            "Original schema: {}",
            serde_json::to_string_pretty(&original_json).unwrap()
        );

        // Now test with our transform
        let tool_def = ToolDefinition::from(&tool);
        let schema_json = serde_json::to_value(&tool_def.input_schema).unwrap();

        println!(
            "\nTransformed schema: {}",
            serde_json::to_string_pretty(&schema_json).unwrap()
        );

        // Check that $schema is not present
        assert!(!schema_json.as_object().unwrap().contains_key("$schema"));

        // Check that definitions are not present
        assert!(!schema_json.as_object().unwrap().contains_key("definitions"));

        // Check that the operation field has been simplified to enum format
        let properties = schema_json.get("properties").unwrap();
        let operation = properties.get("operation").unwrap();

        // Should have type: string and enum array
        assert_eq!(operation.get("type").unwrap(), "string");
        let enum_values = operation.get("enum").unwrap().as_array().unwrap();
        assert_eq!(enum_values.len(), 4);
        assert!(enum_values.contains(&serde_json::Value::String("prepend".to_string())));
        assert!(enum_values.contains(&serde_json::Value::String("append".to_string())));
        assert!(enum_values.contains(&serde_json::Value::String("replace".to_string())));
        assert!(enum_values.contains(&serde_json::Value::String("swap".to_string())));
    }
}
