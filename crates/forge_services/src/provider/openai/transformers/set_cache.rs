use forge_app::domain::Transformer;

use super::super::request::{Request, Role};

/// Transformer that caches the last user/system message for supported models
pub struct SetCache;

impl Transformer for SetCache {
    type Value = Request;

    /// Caches the last 2 eligible messages to optimize API performance. System
    /// messages are always eligible, User messages are eligible (but
    /// consecutive User messages are consolidated to only the last one),
    /// and Assistant messages are never cached but reset User message
    /// sequences.
    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(messages) = request.messages.as_mut() {
            let mut last_was_user = false;
            let mut cache_positions = Vec::new();
            for (i, message) in messages.iter().enumerate() {
                if message.role == Role::User {
                    if last_was_user {
                        cache_positions.pop();
                    }
                    cache_positions.push(i);
                    last_was_user = true;
                } else if message.role == Role::Assistant {
                    last_was_user = false;
                } else if message.role == Role::System {
                    cache_positions.push(i);
                    last_was_user = false;
                }
            }

            for pos in cache_positions.into_iter().rev().take(2) {
                if let Some(ref content) = messages[pos].content {
                    messages[pos].content = Some(content.clone().cached());
                }
            }

            request
        } else {
            request
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use forge_app::domain::{Context, ContextMessage, ModelId, Role, TextMessage};
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
                        model: ModelId::new("gpt-4").into(),
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
        let mut transformer = SetCache;
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
    fn test_transformation() {
        let actual = create_test_context("suu");
        let expected = "[su[u";
        assert_eq!(actual, expected);

        let actual = create_test_context("suua");
        let expected = "[su[ua";
        assert_eq!(actual, expected);

        let actual = create_test_context("suuau");
        let expected = "su[ua[u";
        assert_eq!(actual, expected);

        let actual = create_test_context("suuauu");
        let expected = "su[uau[u";
        assert_eq!(actual, expected);

        let actual = create_test_context("suuauuaaau");
        let expected = "suuau[uaaa[u";
        assert_eq!(actual, expected);

        let actual = create_test_context("suuauuaaauauau");
        let expected = "suuauuaaaua[ua[u";
        assert_eq!(actual, expected);

        let actual = create_test_context("suuaaaaaaaaaaa");
        let expected = "[su[uaaaaaaaaaaa";
        assert_eq!(actual, expected);
    }
}
