use forge_domain::Transformer;

use crate::dto::openai::Request;

/// Transformer that implements a Qwen-specific cache strategy:
/// - Only caches the last message in the conversation
/// - This is based on Qwen model documentation requirements
pub struct QwenSetCache;

impl Transformer for QwenSetCache {
    type Value = Request;

    /// Implements a Qwen-specific cache strategy:
    /// 1. Cache only the last message (index messages.len() - 1)
    /// 2. Remove cache control from all other messages
    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(messages) = request.messages.as_mut() {
            let len = messages.len();

            if len == 0 {
                return request;
            }

            // Remove cache control from all messages first
            for message in messages.iter_mut() {
                if let Some(ref content) = message.content {
                    message.content = Some(content.clone().cached(false));
                }
            }

            // Add cache control only to the last message
            if let Some(message) = messages.last_mut()
                && let Some(ref content) = message.content
            {
                message.content = Some(content.clone().cached(true));
            }
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use forge_domain::{Context, ContextMessage, ModelId, Role, TextMessage};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_context(message: impl ToString) -> String {
        let context = Context {
            conversation_id: None,
            messages: message
                .to_string()
                .chars()
                .map(|c| match c {
                    's' => ContextMessage::Text(TextMessage {
                        role: Role::System,
                        content: c.to_string(),
                        tool_calls: None,
                        model: None,
                        reasoning_details: None,
                    }),
                    'u' => ContextMessage::Text(TextMessage {
                        role: Role::User,
                        content: c.to_string(),
                        tool_calls: None,
                        model: ModelId::new("qwen/qwen3-235b-a22b").into(),
                        reasoning_details: None,
                    }),
                    'a' => ContextMessage::Text(TextMessage {
                        role: Role::Assistant,
                        content: c.to_string(),
                        tool_calls: None,
                        model: None,
                        reasoning_details: None,
                    }),
                    _ => {
                        panic!("Invalid character in test message");
                    }
                })
                .collect(),
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        let mut transformer = QwenSetCache;
        let request = transformer.transform(request);
        let mut output = String::new();
        let sequences = request
            .messages
            .into_iter()
            .flatten()
            .flat_map(|m| m.content)
            .enumerate()
            .filter(|(_, m)| m.is_cached())
            .map(|(i, _)| i)
            .collect::<HashSet<usize>>();

        for (i, c) in message.to_string().chars().enumerate() {
            if sequences.contains(&i) {
                output.push('[');
            }
            output.push_str(c.to_string().as_str())
        }

        output
    }

    #[test]
    fn test_single_message() {
        let actual = create_test_context("s");
        let expected = "[s";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_two_messages() {
        let actual = create_test_context("su");
        let expected = "s[u";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_three_messages_only_last_cached() {
        let actual = create_test_context("sua");
        let expected = "su[a";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_four_messages_only_last_cached() {
        let actual = create_test_context("suau");
        let expected = "sua[u";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_five_messages_only_last_cached() {
        let actual = create_test_context("suaua");
        let expected = "suau[a";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_longer_conversation() {
        let actual = create_test_context("suuauuaaau");
        let expected = "suuauuaaa[u";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_conversation() {
        let context = Context {
            conversation_id: None,
            messages: vec![],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        let mut transformer = QwenSetCache;
        let result = transformer.transform(request);

        // Should not panic and should return empty messages
        assert!(result.messages.unwrap_or_default().is_empty());
    }

    #[test]
    fn test_message_without_content() {
        let context = Context {
            conversation_id: None,
            messages: vec![
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "first".to_string(),
                    tool_calls: None,
                    model: ModelId::new("qwen/qwen3-235b-a22b").into(),
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::Assistant,
                    content: "last".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
            ],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        let mut transformer = QwenSetCache;
        let result = transformer.transform(request);

        let messages = result.messages.unwrap();
        assert_eq!(messages.len(), 2);

        // First message should not be cached
        assert!(!messages[0].content.as_ref().unwrap().is_cached());

        // Last message should be cached
        assert!(messages[1].content.as_ref().unwrap().is_cached());
    }
}
