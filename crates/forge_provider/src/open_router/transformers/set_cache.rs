use crate::open_router::request::{OpenRouterRequest, OpenRouterRole};
use crate::open_router::transformers::Transformer;

/// Transformer that caches the last user/system message for supported models
pub struct SetCache;

impl Transformer for SetCache {
    fn transform(&self, mut request: OpenRouterRequest) -> OpenRouterRequest {
        if let Some(messages) = request.messages.as_mut() {
            let mut last_was_user = false;
            let mut cache_positions = Vec::new();
            for (i, message) in messages.iter().enumerate() {
                if message.role == OpenRouterRole::User {
                    if !last_was_user {
                        cache_positions.push(i);
                    }
                    last_was_user = true;
                } else if message.role == OpenRouterRole::Assistant
                    || message.role == OpenRouterRole::System
                {
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
    use forge_domain::{ContentMessage, Context, ContextMessage, Role};

    use super::*;
    use pretty_assertions::assert_eq;

    fn create_test_context(message: impl ToString) -> Vec<usize> {
        let context = Context {
            messages: message
                .to_string()
                .chars()
                .into_iter()
                .map(|c| match c {
                    's' => ContextMessage::ContentMessage(ContentMessage {
                        role: Role::System,
                        content: c.to_string(),
                        tool_calls: None,
                    }),
                    'u' => ContextMessage::ContentMessage(ContentMessage {
                        role: Role::User,
                        content: c.to_string(),
                        tool_calls: None,
                    }),
                    'a' => ContextMessage::ContentMessage(ContentMessage {
                        role: Role::Assistant,
                        content: c.to_string(),
                        tool_calls: None,
                    }),
                    _ => {
                        panic!("Invalid character in test message");
                    }
                })
                .collect::<Vec<_>>(),
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
        };

        let request = OpenRouterRequest::from(context);
        let request = SetCache.transform(request);
        request
            .messages
            .into_iter()
            .flatten()
            .flat_map(|m| m.content)
            .enumerate()
            .filter(|(_, m)| m.is_cached())
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    }

    #[test]
    fn test_transformation() {
        let actual = create_test_context("suu");
        let expected = vec![1];
        assert_eq!(actual, expected);

        let actual = create_test_context("suua");
        let expected = vec![1];
        assert_eq!(actual, expected);

        let actual = create_test_context("suuau");
        let expected = vec![1, 4];
        assert_eq!(actual, expected);

        let actual = create_test_context("suuauu");
        let expected = vec![1, 4];
        assert_eq!(actual, expected);

        let actual = create_test_context("suuauuaaau");
        let expected = vec![4, 9];
        assert_eq!(actual, expected);
    }
}
