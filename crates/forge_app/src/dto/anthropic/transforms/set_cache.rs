use forge_domain::Transformer;

use crate::dto::anthropic::Request;

/// Transformer that implements a simple two-breakpoint cache strategy:
/// - Always caches the first message in the conversation
/// - Always caches the last message in the conversation
/// - Removes cache control from the second-to-last message
pub struct SetCache;

impl Transformer for SetCache {
    type Value = Request;

    /// Implements a simple two-breakpoint cache strategy:
    /// 1. Cache the first system message as it should be static.
    /// 2. Cache the last message (index messages.len() - 1)
    /// 3. Remove cache control from second-to-last message (index
    ///    messages.len() - 2)
    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        let len = request.get_messages().len();
        let sys_len = request.system.as_ref().map_or(0, |msgs| msgs.len());

        if len == 0 && sys_len == 0 {
            return request;
        }

        // Cache the very first system message, ideally you should keep static content
        // in it.
        if let Some(system_messages) = request.system.as_mut()
            && let Some(first_message) = system_messages.first_mut() {
                *first_message = std::mem::take(first_message).cached(true);
            }

        // Add cache control to last message (if different from first)
        if let Some(message) = request.get_messages_mut().last_mut() {
            *message = std::mem::take(message).cached(true);
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
                        model: ModelId::new("claude-3-5-sonnet-20241022").into(),
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

        let request = Request::try_from(context).expect("Failed to convert context to request");
        let mut transformer = SetCache;
        let request = transformer.transform(request);
        let mut output = String::new();
        let sequences = request
            .get_messages()
            .iter()
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
        let actual = create_test_context("u");
        let expected = "[u";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_two_messages() {
        let actual = create_test_context("ua");
        let expected = "[u[a";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_three_messages_first_and_last_cached() {
        let actual = create_test_context("uau");
        let expected = "[ua[u";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_four_messages_first_and_last_cached() {
        let actual = create_test_context("uaua");
        let expected = "[uau[a";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_five_messages_first_and_last_cached() {
        let actual = create_test_context("uauau");
        let expected = "[uaua[u";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_longer_conversation() {
        let actual = create_test_context("uauauauaua");
        let expected = "[uauauauau[a";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cache_removal_from_second_to_last() {
        // Test that second-to-last message doesn't have cache when there are 3+
        // messages
        let actual = create_test_context("uauauauauauaua");
        let expected = "[uauauauauauau[a";
        assert_eq!(actual, expected);
    }
}
