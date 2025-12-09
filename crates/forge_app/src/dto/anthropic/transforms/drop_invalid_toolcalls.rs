use forge_domain::Transformer;
use serde_json::json;

use crate::dto::anthropic::{Content, Request};

/// Transformer that normalizes ToolUse content to ensure inputs are always
/// objects.
/// - Preserves `Content::Text` and other content types as-is
/// - For `Content::ToolUse`:
///   - If input is already an object, keeps it unchanged
///   - If input is None, keeps it as None
///   - If input is non-object (string, array, number, etc.), wraps it in
///     `{"json": value}`
pub struct DropInvalidToolUse;

impl Transformer for DropInvalidToolUse {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        for message in request.get_messages_mut() {
            for content in &mut message.content {
                if let Content::ToolUse { input, .. } = content {
                    *input = match input.take() {
                        Some(value) if value.is_object() => Some(value),
                        Some(value) => Some(json!({ "json": value })),
                        None => None,
                    };
                }
            }
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Context, ContextMessage, ModelId, Role, TextMessage, ToolCallArguments, ToolCallFull,
        Transformer,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::dto::anthropic::Request;

    fn transform_tool_call(json_args: &str) -> Request {
        let fixture = Context::default().messages(vec![ContextMessage::Text(
            TextMessage::new(Role::User, "Hello")
                .tool_calls(vec![
                    ToolCallFull::new("test_tool")
                        .call_id("call_123")
                        .arguments(ToolCallArguments::from_json(json_args)),
                ])
                .model(ModelId::new("claude-3-5-sonnet-20241022")),
        )
        .into()]);
        DropInvalidToolUse.transform(Request::try_from(fixture).unwrap())
    }

    fn get_tool_input(request: &Request) -> &Option<serde_json::Value> {
        if let Content::ToolUse { input, .. } = &request.messages[0].content[1] {
            input
        } else {
            panic!("Expected ToolUse content")
        }
    }

    #[test]
    fn test_preserves_tool_use_with_object_input() {
        let actual = transform_tool_call(r#"{"key": "value"}"#);
        assert_eq!(get_tool_input(&actual), &Some(json!({"key": "value"})));
    }

    #[test]
    fn test_wraps_tool_use_with_string_input() {
        let actual = transform_tool_call(r#""string_value""#);
        assert_eq!(
            get_tool_input(&actual),
            &Some(json!({"json": "string_value"}))
        );
    }

    #[test]
    fn test_wraps_tool_use_with_array_input() {
        let actual = transform_tool_call(r#"[1, 2, 3]"#);
        assert_eq!(get_tool_input(&actual), &Some(json!({"json": [1, 2, 3]})));
    }

    #[test]
    fn test_wraps_tool_use_with_number_input() {
        let actual = transform_tool_call(r#"42"#);
        assert_eq!(get_tool_input(&actual), &Some(json!({"json": 42})));
    }

    #[test]
    fn test_wraps_tool_use_with_none_input() {
        let request = Request::default().messages(vec![crate::dto::anthropic::Message {
            role: crate::dto::anthropic::Role::User,
            content: vec![Content::ToolUse {
                id: "call_123".to_string(),
                name: "test_tool".to_string(),
                input: None,
                cache_control: None,
            }],
        }]);
        let actual = DropInvalidToolUse.transform(request);

        if let Content::ToolUse { input, .. } = &actual.messages[0].content[0] {
            assert_eq!(input, &None);
        }
    }

    #[test]
    fn test_empty_messages_remain_empty() {
        let actual = DropInvalidToolUse.transform(Request::try_from(Context::default()).unwrap());
        assert_eq!(actual.messages.len(), 0);
    }

    #[test]
    fn test_preserves_text_content() {
        let fixture = Context::default().messages(vec![ContextMessage::Text(TextMessage::new(
            Role::User,
            "Hello",
        ))
        .into()]);
        let actual = DropInvalidToolUse.transform(Request::try_from(fixture).unwrap());

        assert_eq!(actual.messages.len(), 1);
        assert!(matches!(
            actual.messages[0].content[0],
            Content::Text { .. }
        ));
    }
}
