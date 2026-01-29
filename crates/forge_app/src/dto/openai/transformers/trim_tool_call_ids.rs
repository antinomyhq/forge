use forge_domain::Transformer;

use crate::dto::openai::Request;

/// Trims tool call IDs to a maximum of 40 characters for OpenAI compatibility.
/// OpenAI requires tool call IDs to be max 40 characters.
pub struct TrimToolCallIds;

impl Transformer for TrimToolCallIds {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(messages) = request.messages.as_mut() {
            for message in messages.iter_mut() {
                // Trim tool_call_id in tool role messages
                if let Some(ref mut tool_call_id) = message.tool_call_id {
                    *tool_call_id = forge_domain::ToolCallId::new(
                        tool_call_id.as_str().chars().take(40).collect::<String>(),
                    );
                }

                // Trim tool call IDs in assistant messages
                if let Some(ref mut id) = message.tool_calls {
                    for tool_call in id.iter_mut() {
                        if let Some(ref mut tool_call_id) = tool_call.id {
                            let trimmed_id =
                                tool_call_id.as_str().chars().take(40).collect::<String>();
                            *tool_call_id = forge_domain::ToolCallId::new(trimmed_id);
                        }
                    }
                }
            }
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::dto::openai::response::{FunctionCall, ToolCall as ResponseToolCall};
    use crate::dto::openai::tool_choice::FunctionType;
    use crate::dto::openai::{Message, Role};

    #[test]
    fn test_trim_tool_call_id_in_tool_message() {
        // Create a tool call ID that's longer than 40 characters
        let long_id = "call_12345678901234567890123456789012345678901234567890";
        assert!(long_id.len() > 40);

        let fixture = Request::default().messages(vec![Message {
            role: Role::Tool,
            content: None,
            name: None,
            tool_call_id: Some(forge_domain::ToolCallId::new(long_id)),
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
        }]);

        let actual = TrimToolCallIds.transform(fixture);

        let expected_id = "call_12345678901234567890123456789012345";
        assert_eq!(expected_id.len(), 40);

        let messages = actual.messages.unwrap();
        assert_eq!(
            messages[0].tool_call_id.as_ref().unwrap().as_str(),
            expected_id
        );
    }

    #[test]
    fn test_trim_tool_call_id_in_assistant_message() {
        // Create tool calls with IDs longer than 40 characters
        let long_id = "call_12345678901234567890123456789012345678901234567890";
        assert!(long_id.len() > 40);

        let fixture = Request::default().messages(vec![Message {
            role: Role::Assistant,
            content: None,
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![ResponseToolCall {
                id: Some(forge_domain::ToolCallId::new(long_id)),
                r#type: FunctionType,
                function: FunctionCall {
                    name: Some(forge_domain::ToolName::new("test_tool")),
                    arguments: "{}".to_string(),
                },
            }]),
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
        }]);

        let actual = TrimToolCallIds.transform(fixture);

        let expected_id = "call_12345678901234567890123456789012345";
        assert_eq!(expected_id.len(), 40);

        let messages = actual.messages.unwrap();
        assert_eq!(
            messages[0].tool_calls.as_ref().unwrap()[0]
                .id
                .as_ref()
                .unwrap()
                .as_str(),
            expected_id
        );
    }

    #[test]
    fn test_trim_multiple_tool_calls_in_assistant_message() {
        let long_id_1 = "call_11111111111111111111111111111111111111111111111111";
        let long_id_2 = "call_22222222222222222222222222222222222222222222222222";
        assert!(long_id_1.len() > 40);
        assert!(long_id_2.len() > 40);

        let fixture = Request::default().messages(vec![Message {
            role: Role::Assistant,
            content: None,
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![
                ResponseToolCall {
                    id: Some(forge_domain::ToolCallId::new(long_id_1)),
                    r#type: FunctionType,
                    function: FunctionCall {
                        name: Some(forge_domain::ToolName::new("tool_1")),
                        arguments: "{}".to_string(),
                    },
                },
                ResponseToolCall {
                    id: Some(forge_domain::ToolCallId::new(long_id_2)),
                    r#type: FunctionType,
                    function: FunctionCall {
                        name: Some(forge_domain::ToolName::new("tool_2")),
                        arguments: "{}".to_string(),
                    },
                },
            ]),
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
        }]);

        let actual = TrimToolCallIds.transform(fixture);

        let expected_id_1 = "call_11111111111111111111111111111111111";
        let expected_id_2 = "call_22222222222222222222222222222222222";
        assert_eq!(expected_id_1.len(), 40);
        assert_eq!(expected_id_2.len(), 40);

        let messages = actual.messages.unwrap();
        let tool_calls = messages[0].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id.as_ref().unwrap().as_str(), expected_id_1);
        assert_eq!(tool_calls[1].id.as_ref().unwrap().as_str(), expected_id_2);
    }

    #[test]
    fn test_trim_does_not_affect_short_ids() {
        // Create a tool call ID that's already under 40 characters
        let short_id = "call_123";
        assert!(short_id.len() < 40);

        let fixture = Request::default().messages(vec![Message {
            role: Role::Tool,
            content: None,
            name: None,
            tool_call_id: Some(forge_domain::ToolCallId::new(short_id)),
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
        }]);

        let actual = TrimToolCallIds.transform(fixture);

        let messages = actual.messages.unwrap();
        assert_eq!(
            messages[0].tool_call_id.as_ref().unwrap().as_str(),
            short_id
        );
    }

    #[test]
    fn test_trim_exactly_40_chars_id() {
        // Create a tool call ID that's exactly 40 characters
        let exact_id = "call_12345678901234567890123456789012345";
        assert_eq!(exact_id.len(), 40);

        let fixture = Request::default().messages(vec![Message {
            role: Role::Tool,
            content: None,
            name: None,
            tool_call_id: Some(forge_domain::ToolCallId::new(exact_id)),
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
        }]);

        let actual = TrimToolCallIds.transform(fixture);

        let messages = actual.messages.unwrap();
        assert_eq!(
            messages[0].tool_call_id.as_ref().unwrap().as_str(),
            exact_id
        );
    }

    #[test]
    fn test_trim_handles_multiple_messages() {
        let long_id = "call_12345678901234567890123456789012345678901234567890";
        let short_id = "call_abc";

        let fixture = Request::default().messages(vec![
            Message {
                role: Role::Tool,
                content: None,
                name: None,
                tool_call_id: Some(forge_domain::ToolCallId::new(long_id)),
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
            },
            Message {
                role: Role::Tool,
                content: None,
                name: None,
                tool_call_id: Some(forge_domain::ToolCallId::new(short_id)),
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
            },
        ]);

        let actual = TrimToolCallIds.transform(fixture);

        let messages = actual.messages.unwrap();
        assert_eq!(
            messages[0].tool_call_id.as_ref().unwrap().as_str().len(),
            40
        );
        assert_eq!(
            messages[1].tool_call_id.as_ref().unwrap().as_str().len(),
            short_id.len()
        );
    }
}
