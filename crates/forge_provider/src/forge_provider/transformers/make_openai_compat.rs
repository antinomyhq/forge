use forge_domain::Transformer;

use crate::forge_provider::request::Request;

/// makes the Request compatible with the OpenAI API.
pub struct MakeOpenAiCompat;

impl Transformer for MakeOpenAiCompat {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        // remove fields that are not supported by open-ai.
        request.provider = None;
        request.transforms = None;
        request.prompt = None;
        request.models = None;
        request.route = None;
        request.top_k = None;
        request.top_p = None;
        request.repetition_penalty = None;
        request.min_p = None;
        request.top_a = None;
        request.session_id = None;

        let tools_present = request
            .tools
            .as_ref()
            .is_some_and(|tools| !tools.is_empty());

        if !tools_present {
            // drop `parallel_tool_calls` field if tools are not passed to the request.
            request.parallel_tool_calls = None;
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parallel_tool_calls_removed_when_no_tools() {
        let fixture = Request::default().parallel_tool_calls(true);
        let mut transformer = MakeOpenAiCompat;
        let actual = transformer.transform(fixture);
        let expected = None;
        assert_eq!(actual.parallel_tool_calls, expected);
    }

    #[test]
    fn test_parallel_tool_calls_removed_when_empty_tools() {
        let fixture = Request::default().tools(vec![]).parallel_tool_calls(true);
        let mut transformer = MakeOpenAiCompat;
        let actual = transformer.transform(fixture);
        let expected = None;
        assert_eq!(actual.parallel_tool_calls, expected);
    }

    #[test]
    fn test_parallel_tool_calls_preserved_when_tools_present() {
        let fixture = Request::default()
            .tools(vec![crate::forge_provider::request::Tool {
                r#type: crate::forge_provider::tool_choice::FunctionType,
                function: crate::forge_provider::request::FunctionDescription {
                    description: Some("test".to_string()),
                    name: "test".to_string(),
                    parameters: serde_json::json!({}),
                },
            }])
            .parallel_tool_calls(true);
        let mut transformer = MakeOpenAiCompat;
        let actual = transformer.transform(fixture);
        let expected = Some(true);
        assert_eq!(actual.parallel_tool_calls, expected);
    }
}
