use forge_domain::Transformer;

use crate::dto::openai::Request;

/// Normalizes tool schemas for OpenAI compatibility
///
/// Ensures tool parameter schemas meet OpenAI's requirements:
/// - Removes top-level "description" and "title" fields from parameters
/// - Adds empty "properties" object for type="object" if missing
pub struct NormalizeToolSchema;

impl Transformer for NormalizeToolSchema {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(tools) = request.tools.as_mut() {
            for tool in tools.iter_mut() {
                if let Some(obj) = tool.function.parameters.as_object_mut() {
                    // Remove tool usage description and title from parameters property
                    obj.remove("description");
                    obj.remove("title");
                }
            }
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::dto::openai::{FunctionDescription, FunctionType, Tool};

    #[test]
    fn test_normalize_removes_description_and_title() {
        let fixture = Request::default().tools(vec![Tool {
            r#type: FunctionType,
            function: FunctionDescription {
                name: "test_tool".to_string(),
                description: Some("Test tool description".to_string()),
                parameters: json!({
                    "type": "object",
                    "description": "Schema description",
                    "title": "Schema title",
                    "properties": {
                        "param1": {"type": "string"}
                    }
                }),
            },
        }]);

        let mut transformer = NormalizeToolSchema;
        let actual = transformer.transform(fixture);

        let expected_params = json!({
            "type": "object",
            "properties": {
                "param1": {"type": "string"}
            }
        });

        assert_eq!(
            actual.tools.unwrap()[0].function.parameters,
            expected_params
        );
    }

    #[test]
    fn test_normalize_adds_empty_properties_when_missing() {
        let fixture = Request::default().tools(vec![Tool {
            r#type: FunctionType,
            function: FunctionDescription {
                name: "test_tool".to_string(),
                description: Some("Test tool description".to_string()),
                parameters: json!({
                    "type": "object",
                }),
            },
        }]);

        let mut transformer = NormalizeToolSchema;
        let actual = transformer.transform(fixture);

        let expected_params = json!({
            "type": "object",
            "properties": {}
        });

        assert_eq!(
            actual.tools.unwrap()[0].function.parameters,
            expected_params
        );
    }

    #[test]
    fn test_normalize_preserves_existing_properties() {
        let fixture = Request::default().tools(vec![Tool {
            r#type: FunctionType,
            function: FunctionDescription {
                name: "test_tool".to_string(),
                description: Some("Test tool description".to_string()),
                parameters: json!({
                    "type": "object",
                    "description": "Should be removed",
                    "properties": {
                        "existing": {"type": "number"}
                    }
                }),
            },
        }]);

        let mut transformer = NormalizeToolSchema;
        let actual = transformer.transform(fixture);

        let expected_params = json!({
            "type": "object",
            "properties": {
                "existing": {"type": "number"}
            }
        });

        assert_eq!(
            actual.tools.unwrap()[0].function.parameters,
            expected_params
        );
    }

    #[test]
    fn test_normalize_skips_non_object_types() {
        let fixture = Request::default().tools(vec![Tool {
            r#type: FunctionType,
            function: FunctionDescription {
                name: "test_tool".to_string(),
                description: Some("Test tool description".to_string()),
                parameters: json!({
                    "type": "string",
                }),
            },
        }]);

        let mut transformer = NormalizeToolSchema;
        let actual = transformer.transform(fixture);

        let expected_params = json!({
            "type": "string",
        });

        assert_eq!(
            actual.tools.unwrap()[0].function.parameters,
            expected_params
        );
    }
}
