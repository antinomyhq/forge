use forge_domain::Transformer;

use crate::dto::openai::Request;

/// For gemini3 models: strips thought signatures from the beginning up to (and
/// including) the last message that does not have a thought signature.
///
/// This transformer scans messages from start to end, finds the last index
/// where a message lacks a thought signature, and strips thought signatures
/// from all messages at or before that index. Messages after that point retain
/// their thought signatures.
///
/// Example:
/// - Message 1: has signature
/// - Message 2: has signature
/// - Message 3: no signature
/// - Message 4: has signature
/// - Message 5: no signature  <-- last message without signature
/// - Message 6: has signature
/// - Message 7: has signature
///
/// Result: Strip signatures from messages 1-5, keep signatures for messages
/// 6-7.
pub struct StripThoughtSignatureForGemini3;

impl Transformer for StripThoughtSignatureForGemini3 {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        let Some(messages) = request.messages.as_mut() else {
            return request;
        };

        if messages.is_empty() {
            return request;
        }

        // Find the last index where a message lacks a thought signature
        let last_no_signature_index = messages
            .iter()
            .enumerate()
            .filter_map(|(idx, msg)| {
                // Check if message has no thought signature
                let has_no_signature = msg
                    .extra_content
                    .as_ref()
                    .and_then(|ec| ec.google.as_ref())
                    .and_then(|g| g.thought_signature.as_ref())
                    .is_none();

                if has_no_signature { Some(idx) } else { None }
            })
            .next_back();

        // If we found a message without a signature, strip signatures from all messages
        // up to and including that index
        if let Some(last_idx) = last_no_signature_index {
            for msg in messages.iter_mut().take(last_idx + 1) {
                // Remove extra_content entirely from the message
                msg.extra_content = None;

                // Also remove extra_content from tool_calls
                if let Some(ref mut tool_calls) = msg.tool_calls {
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
    use forge_domain::Transformer;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::dto::openai::{
        ExtraContent, FunctionCall, FunctionType, GoogleMetadata, Message, MessageContent, Role,
        ToolCall,
    };

    fn create_message_with_signature(idx: usize) -> Message {
        Message {
            role: Role::Assistant,
            content: Some(MessageContent::Text(format!("Message {}", idx))),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            extra_content: Some(ExtraContent {
                google: Some(GoogleMetadata { thought_signature: Some(format!("sig{}", idx)) }),
            }),
        }
    }

    fn create_message_without_signature(idx: usize) -> Message {
        Message {
            role: Role::Assistant,
            content: Some(MessageContent::Text(format!("Message {}", idx))),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            extra_content: None,
        }
    }

    fn has_signature(msg: &Message) -> bool {
        msg.extra_content.is_some()
    }

    #[test]
    fn test_strip_signatures_up_to_last_no_signature() {
        // Example from requirements:
        // 1 has signature
        // 2 has signature
        // 3 no signature
        // 4 has signature
        // 5 no signature  <-- last without signature
        // 6 has signature
        // 7 has signature

        let fixture = Request::default().messages(vec![
            create_message_with_signature(1),
            create_message_with_signature(2),
            create_message_without_signature(3),
            create_message_with_signature(4),
            create_message_without_signature(5),
            create_message_with_signature(6),
            create_message_with_signature(7),
        ]);

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();

        // Messages 1-5 should have signatures stripped
        assert!(
            !has_signature(&messages[0]),
            "Message 1 should not have signature"
        );
        assert!(
            !has_signature(&messages[1]),
            "Message 2 should not have signature"
        );
        assert!(
            !has_signature(&messages[2]),
            "Message 3 should not have signature (already none)"
        );
        assert!(
            !has_signature(&messages[3]),
            "Message 4 should not have signature"
        );
        assert!(
            !has_signature(&messages[4]),
            "Message 5 should not have signature (already none)"
        );

        // Messages 6-7 should retain signatures
        assert!(
            has_signature(&messages[5]),
            "Message 6 should have signature"
        );
        assert!(
            has_signature(&messages[6]),
            "Message 7 should have signature"
        );

        // Verify the actual signature values are preserved for 6 and 7
        assert_eq!(
            messages[5]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig6".to_string())
        );
        assert_eq!(
            messages[6]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig7".to_string())
        );
    }

    #[test]
    fn test_no_messages_without_signature() {
        // All messages have signatures - nothing should be stripped
        let fixture = Request::default().messages(vec![
            create_message_with_signature(1),
            create_message_with_signature(2),
            create_message_with_signature(3),
        ]);

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();

        // All signatures should be preserved
        assert!(has_signature(&messages[0]));
        assert!(has_signature(&messages[1]));
        assert!(has_signature(&messages[2]));
    }

    #[test]
    fn test_all_messages_without_signature() {
        // No messages have signatures - nothing changes
        let fixture = Request::default().messages(vec![
            create_message_without_signature(1),
            create_message_without_signature(2),
            create_message_without_signature(3),
        ]);

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();

        // All messages should still have no signatures
        assert!(!has_signature(&messages[0]));
        assert!(!has_signature(&messages[1]));
        assert!(!has_signature(&messages[2]));
    }

    #[test]
    fn test_last_message_without_signature_is_last() {
        // Last message (index 2) has no signature - all signatures stripped
        let fixture = Request::default().messages(vec![
            create_message_with_signature(1),
            create_message_with_signature(2),
            create_message_without_signature(3),
        ]);

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();

        // All messages should have no signatures now
        assert!(!has_signature(&messages[0]));
        assert!(!has_signature(&messages[1]));
        assert!(!has_signature(&messages[2]));
    }

    #[test]
    fn test_first_message_without_signature() {
        // First message has no signature - strip only from message 0
        let fixture = Request::default().messages(vec![
            create_message_without_signature(1),
            create_message_with_signature(2),
            create_message_with_signature(3),
        ]);

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();

        // Message 1 should have no signature (already none)
        assert!(!has_signature(&messages[0]));

        // Messages 2 and 3 should retain signatures
        assert!(has_signature(&messages[1]));
        assert!(has_signature(&messages[2]));
    }

    #[test]
    fn test_empty_messages() {
        let fixture = Request::default();

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        assert!(actual.messages.is_none());
    }

    #[test]
    fn test_strips_from_tool_calls_too() {
        let mut msg_with_tool = create_message_with_signature(1);
        msg_with_tool.tool_calls = Some(vec![ToolCall {
            id: None,
            r#type: FunctionType,
            function: FunctionCall { name: None, arguments: "{}".to_string() },
            extra_content: Some(ExtraContent {
                google: Some(GoogleMetadata { thought_signature: Some("tool_sig".to_string()) }),
            }),
        }]);

        let fixture = Request::default().messages(vec![
            msg_with_tool,
            create_message_without_signature(2),
            create_message_with_signature(3),
        ]);

        let mut transformer = StripThoughtSignatureForGemini3;
        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();

        // Message 1 should have signature stripped (including tool call)
        assert!(!has_signature(&messages[0]));
        let tool_calls = messages[0].tool_calls.as_ref().unwrap();
        assert!(tool_calls[0].extra_content.is_none());

        // Message 2 has no signature
        assert!(!has_signature(&messages[1]));

        // Message 3 should retain signature
        assert!(has_signature(&messages[2]));
    }
}
