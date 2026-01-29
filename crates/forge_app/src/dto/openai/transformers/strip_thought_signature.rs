use forge_domain::Transformer;

use crate::dto::openai::Request;

/// Strips thought signatures from request messages.
///
/// This transformer removes the `extra_content` field from all messages,
/// which contains Google-specific thought signatures. This should be applied
/// to models that don't support thought signatures (all models except gemini3).
pub struct StripThoughtSignature;

impl Transformer for StripThoughtSignature {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(messages) = request.messages.as_mut() {
            for message in messages.iter_mut() {
                // Remove extra_content which contains thought_signature
                message.extra_content = None;

                // Also remove extra_content from tool_calls
                if let Some(tool_calls) = message.tool_calls.as_mut() {
                    for tool_call in tool_calls.iter_mut() {
                        tool_call.extra_content = None;
                    }
                }
            }
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{ModelId, Transformer};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::dto::openai::{
        ExtraContent, FunctionCall, FunctionType, GoogleMetadata, Message, MessageContent, Role,
        ToolCall,
    };

    #[test]
    fn test_strip_thought_signature_removes_extra_content() {
        let fixture = Request::default().messages(vec![Message {
            role: Role::Assistant,
            content: Some(MessageContent::Text("Hello".to_string())),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            extra_content: Some(ExtraContent {
                google: Some(GoogleMetadata {
                    thought_signature: Some("sig123".to_string()),
                }),
            }),
        }]);

        let mut transformer = StripThoughtSignature;
        let actual = transformer.transform(fixture);

        assert!(actual.messages.unwrap()[0].extra_content.is_none());
    }

    #[test]
    fn test_strip_thought_signature_removes_from_tool_calls() {
        let fixture = Request::default().messages(vec![Message {
            role: Role::Assistant,
            content: Some(MessageContent::Text("Using tool".to_string())),
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: None,
                r#type: FunctionType,
                function: FunctionCall {
                    name: None,
                    arguments: "{}".to_string(),
                },
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata {
                        thought_signature: Some("sig456".to_string()),
                    }),
                }),
            }]),
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            extra_content: None,
        }]);

        let mut transformer = StripThoughtSignature;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();
        let tool_calls = messages[0].tool_calls.as_ref().unwrap();
        assert!(tool_calls[0].extra_content.is_none());
    }

    #[test]
    fn test_strip_thought_signature_no_messages() {
        let fixture = Request::default();

        let mut transformer = StripThoughtSignature;
        let actual = transformer.transform(fixture);

        assert!(actual.messages.is_none());
    }

    #[test]
    fn test_strip_thought_signature_preserves_other_fields() {
        let fixture = Request::default()
            .model(ModelId::new("gpt-4"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: Some("reasoning".to_string()),
                reasoning_opaque: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata {
                        thought_signature: Some("sig123".to_string()),
                    }),
                }),
            }]);

        let mut transformer = StripThoughtSignature;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_none());
        assert_eq!(messages[0].reasoning_text, Some("reasoning".to_string()));
        assert_eq!(actual.model, Some(ModelId::new("gpt-4")));
    }
}
