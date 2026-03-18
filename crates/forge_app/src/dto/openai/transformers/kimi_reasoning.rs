use forge_domain::Transformer;

use crate::dto::openai::Request;

/// Transformer that converts reasoning details into Kimi's flat
/// `reasoning_content` string format for replayed assistant tool-call messages.
pub struct KimiReasoning;

impl Transformer for KimiReasoning {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(ref mut messages) = request.messages {
            for message in messages.iter_mut() {
                let is_assistant_tool_call = message.role == crate::dto::openai::Role::Assistant
                    && message
                        .tool_calls
                        .as_ref()
                        .is_some_and(|tool_calls| !tool_calls.is_empty());

                if !is_assistant_tool_call {
                    continue;
                }

                if message.reasoning_content.is_some() {
                    continue;
                }

                message.reasoning_content =
                    message.reasoning_details.as_ref().and_then(|details| {
                        details
                            .iter()
                            .find(|detail| detail.r#type == "reasoning.text")
                            .and_then(|detail| detail.text.clone())
                    });
            }
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::dto::openai::response::{FunctionCall, ToolCall};
    use crate::dto::openai::tool_choice::FunctionType;
    use crate::dto::openai::{Message, MessageContent, ReasoningDetail, Request, Role};

    #[test]
    fn test_sets_reasoning_content_for_assistant_tool_call_messages() {
        let fixture = Request::default().messages(vec![Message {
            role: Role::Assistant,
            content: Some(MessageContent::Text(String::new())),
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: Some(forge_domain::ToolCallId::new("call_1")),
                r#type: FunctionType,
                function: FunctionCall {
                    name: Some(forge_domain::ToolName::new("shell")),
                    arguments: "{}".to_string(),
                },
                extra_content: None,
            }]),
            reasoning_details: Some(vec![ReasoningDetail {
                r#type: "reasoning.text".to_string(),
                text: Some("Need to inspect cwd first".to_string()),
                signature: None,
                data: None,
                id: None,
                format: None,
                index: None,
            }]),
            reasoning_text: None,
            reasoning_opaque: None,
            reasoning_content: None,
            extra_content: None,
        }]);

        let actual = KimiReasoning.transform(fixture);
        let actual_message = actual.messages.unwrap().remove(0);

        assert_eq!(
            actual_message.reasoning_content,
            Some("Need to inspect cwd first".to_string())
        );
        assert!(actual_message.reasoning_details.is_some());
    }

    #[test]
    fn test_does_not_set_reasoning_content_for_non_tool_call_messages() {
        let fixture = Request::default().messages(vec![Message {
            role: Role::Assistant,
            content: Some(MessageContent::Text("hello".to_string())),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_details: Some(vec![ReasoningDetail {
                r#type: "reasoning.text".to_string(),
                text: Some("plain reply reasoning".to_string()),
                signature: None,
                data: None,
                id: None,
                format: None,
                index: None,
            }]),
            reasoning_text: None,
            reasoning_opaque: None,
            reasoning_content: None,
            extra_content: None,
        }]);

        let actual = KimiReasoning.transform(fixture);
        let actual_message = actual.messages.unwrap().remove(0);

        assert_eq!(actual_message.reasoning_content, None);
    }
}
