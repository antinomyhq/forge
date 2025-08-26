use forge_domain::Transformer;

use crate::dto::openai::Request;

/// makes the Request compatible with the OpenAI API.
pub struct MakeCerebrasCompat;

impl Transformer for MakeCerebrasCompat {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        // remove fields that are not supported by cerebras.
        request.parallel_tool_calls = None;
        request
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parallel_tool_calls_dropped() {
        use crate::dto::openai::{FunctionDescription, FunctionType, Request, Tool};

        let fixture = Request::default()
            .tools(vec![Tool {
                r#type: FunctionType,
                function: FunctionDescription {
                    description: Some("test".to_string()),
                    name: "test".to_string(),
                    parameters: serde_json::json!({}),
                },
            }])
            .parallel_tool_calls(true);
        let mut transformer = MakeCerebrasCompat;
        let actual = transformer.transform(fixture);
        let expected = None;
        assert_eq!(actual.parallel_tool_calls, expected);
    }
}
