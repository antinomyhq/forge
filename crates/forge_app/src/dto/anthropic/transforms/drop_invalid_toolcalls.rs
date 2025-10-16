use forge_domain::Transformer;

use crate::dto::anthropic::{Content, Request};

/// Transformer that removes content items based on type constraints:
/// - Removes `Content::ToolUse` variants where the input is not an object type
pub struct DropInvalidToolUse;

impl Transformer for DropInvalidToolUse {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        for message in request.get_messages_mut() {
            message.content.retain(|content| match content {
                Content::Text { .. } => true,
                Content::ToolUse { input, .. } => {
                    // Keep only if input is Some and is an object
                    input.as_ref().is_some_and(|v| v.is_object())
                }
                _ => true,
            });
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ContextMessage, ModelId, Role, TextMessage, Transformer};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::dto::anthropic::Request;

    fn create_context_fixture() -> Context {
        Context::default().messages(vec![
            ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "Hello".to_string(),
                tool_calls: None,
                model: ModelId::new("claude-3-5-sonnet-20241022").into(),
                reasoning_details: None,
            }),
            ContextMessage::Text(TextMessage {
                role: Role::Assistant,
                content: "Hi there".to_string(),
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }),
        ])
    }

    #[test]
    fn test_preserves_text_content_from_messages() {
        let fixture = create_context_fixture();
        let request = Request::try_from(fixture).unwrap();
        let mut transformer = DropInvalidToolUse;
        let actual = transformer.transform(request);
        let expected_message_count = 2;
        let expected_content_count = 1; // Text content is preserved

        assert_eq!(actual.messages.len(), expected_message_count);
        assert_eq!(actual.messages[0].content.len(), expected_content_count);
        assert_eq!(actual.messages[1].content.len(), expected_content_count);
        assert!(matches!(
            actual.messages[0].content[0],
            Content::Text { .. }
        ));
        assert!(matches!(
            actual.messages[1].content[0],
            Content::Text { .. }
        ));
    }

    #[test]
    fn test_preserves_tool_use_with_object_input() {
        let fixture = Context::default().messages(vec![ContextMessage::Text(TextMessage {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: Some(vec![forge_domain::ToolCallFull {
                call_id: Some("call_123".into()),
                name: "test_tool".into(),
                arguments: forge_domain::ToolCallArguments::from_json(r#"{"key": "value"}"#),
            }]),
            model: ModelId::new("claude-3-5-sonnet-20241022").into(),
            reasoning_details: None,
        })]);

        let request = Request::try_from(fixture).unwrap();
        let mut transformer = DropInvalidToolUse;
        let actual = transformer.transform(request);

        assert_eq!(actual.messages.len(), 1);
        assert_eq!(actual.messages[0].content.len(), 2); // Text + ToolUse preserved
        assert!(matches!(
            actual.messages[0].content[0],
            Content::Text { .. }
        ));
        assert!(matches!(
            actual.messages[0].content[1],
            Content::ToolUse { .. }
        ));
    }

    #[test]
    fn test_removes_tool_use_with_non_object_input() {
        let fixture = Context::default().messages(vec![ContextMessage::Text(TextMessage {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: Some(vec![forge_domain::ToolCallFull {
                call_id: Some("call_123".into()),
                name: "test_tool".into(),
                arguments: forge_domain::ToolCallArguments::from_json(r#""string_value""#),
            }]),
            model: ModelId::new("claude-3-5-sonnet-20241022").into(),
            reasoning_details: None,
        })]);

        let request = Request::try_from(fixture).unwrap();
        let mut transformer = DropInvalidToolUse;
        let actual = transformer.transform(request);

        assert_eq!(actual.messages.len(), 1);
        assert_eq!(actual.messages[0].content.len(), 1); // Text preserved, ToolUse removed
        assert!(matches!(
            actual.messages[0].content[0],
            Content::Text { .. }
        ));
    }

    #[test]
    fn test_removes_tool_use_with_none_input() {
        use crate::dto::anthropic::{Content, Message, Role};

        let mut request = Request::default();
        request.messages = vec![Message {
            role: Role::User,
            content: vec![Content::ToolUse {
                id: "call_123".to_string(),
                name: "test_tool".to_string(),
                input: None,
                cache_control: None,
            }],
        }];

        let mut transformer = DropInvalidToolUse;
        let actual = transformer.transform(request);

        assert_eq!(actual.messages.len(), 1);
        assert_eq!(actual.messages[0].content.len(), 0);
    }

    #[test]
    fn test_empty_messages_remain_empty() {
        let fixture = Context::default();
        let request = Request::try_from(fixture).unwrap();
        let mut transformer = DropInvalidToolUse;
        let actual = transformer.transform(request);

        assert_eq!(actual.messages.len(), 0);
    }

    #[test]
    fn test_preserves_other_content_types() {
        use crate::dto::anthropic::{Content, ImageSource, Message, Role};

        let mut request = Request::default();
        request.messages = vec![Message {
            role: Role::User,
            content: vec![
                Content::Text { text: "This should be kept".to_string(), cache_control: None },
                Content::Image {
                    source: ImageSource {
                        type_: "base64".to_string(),
                        media_type: Some("image/png".to_string()),
                        data: Some("base64data".to_string()),
                        url: None,
                    },
                    cache_control: None,
                },
                Content::ToolResult {
                    tool_use_id: "call_123".to_string(),
                    content: Some("result".to_string()),
                    is_error: Some(false),
                    cache_control: None,
                },
            ],
        }];

        let mut transformer = DropInvalidToolUse;
        let actual = transformer.transform(request);

        assert_eq!(actual.messages.len(), 1);
        assert_eq!(actual.messages[0].content.len(), 3); // Text, Image, and ToolResult preserved
        assert!(matches!(
            actual.messages[0].content[0],
            Content::Text { .. }
        ));
        assert!(matches!(
            actual.messages[0].content[1],
            Content::Image { .. }
        ));
        assert!(matches!(
            actual.messages[0].content[2],
            Content::ToolResult { .. }
        ));
    }
}
