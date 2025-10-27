use forge_domain::Transformer;

use crate::dto::anthropic::{Request, SystemMessage};

/// Adds authentication system message when OAuth is enabled.
#[derive(Default)]
pub struct AuthSystemMessage;

impl Transformer for AuthSystemMessage {
    type Value = Request;

    /// Prepends auth system message when enabled.
    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        const AUTH_MESSAGE: &str = include_str!("claude_code.md");

        let auth_system_message = SystemMessage {
            r#type: "text".to_string(),
            text: AUTH_MESSAGE.trim().to_string(),
            cache_control: None,
        };

        // Get or create the system messages vector
        let mut system_messages = request.system.take().unwrap_or_default();

        // Prepend the auth message at the beginning
        system_messages.insert(0, auth_system_message);

        request.system = Some(system_messages);
        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ContextMessage, ModelId, Role, TextMessage};
    use pretty_assertions::assert_eq;

    use super::*;

    const AUTH_MESSAGE: &str = include_str!("claude_code.md");

    fn create_request_with_system_messages(system_count: usize) -> Request {
        let mut messages = Vec::new();

        // Add system messages
        for i in 0..system_count {
            messages.push(ContextMessage::Text(TextMessage {
                role: Role::System,
                content: format!("System message {}", i),
                raw_content: None,
                tool_calls: None,
                model: None,
                reasoning_details: None,
            }));
        }

        // Add at least one user message
        messages.push(ContextMessage::Text(TextMessage {
            role: Role::User,
            content: "Hello".to_string(),
            raw_content: None,
            tool_calls: None,
            model: ModelId::new("claude-3-5-sonnet-20241022").into(),
            reasoning_details: None,
        }));

        let context = Context {
            conversation_id: None,
            messages,
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        Request::try_from(context).unwrap()
    }

    #[test]
    fn test_enabled_adds_auth_message() {
        let fixture = create_request_with_system_messages(0);
        let mut transformer = AuthSystemMessage.when(|_| true);

        let actual = transformer.transform(fixture);

        let system_messages = actual.system.unwrap();
        assert_eq!(system_messages.len(), 1);
        assert_eq!(system_messages[0].text, AUTH_MESSAGE.trim());
        assert_eq!(system_messages[0].r#type, "text");
    }

    #[test]
    fn test_disabled_does_not_add_auth_message() {
        let fixture = create_request_with_system_messages(0);
        let mut transformer = AuthSystemMessage.when(|_| false);

        let actual = transformer.transform(fixture);

        // Should have no system messages or empty vector
        let system_messages = actual.system.unwrap_or_default();
        assert_eq!(system_messages.len(), 0);
    }

    #[test]
    fn test_prepends_to_existing_system_messages() {
        let fixture = create_request_with_system_messages(2);
        let mut transformer = AuthSystemMessage.when(|_| true);

        let actual = transformer.transform(fixture);

        let system_messages = actual.system.unwrap();
        assert_eq!(system_messages.len(), 3);

        // Auth message should be first
        assert_eq!(system_messages[0].text, AUTH_MESSAGE.trim());
        assert_eq!(system_messages[0].r#type, "text");

        // Existing messages should follow
        assert_eq!(system_messages[1].text, "System message 0");
        assert_eq!(system_messages[2].text, "System message 1");
    }

    #[test]
    fn test_auth_message_content_matches_file() {
        let fixture = create_request_with_system_messages(0);
        let mut transformer = AuthSystemMessage.when(|_| true);

        let actual = transformer.transform(fixture);

        let system_messages = actual.system.unwrap();
        let expected_content = "You are Claude Code, Anthropic's official CLI for Claude.";
        assert_eq!(system_messages[0].text, expected_content);
    }

    #[test]
    fn test_with_one_existing_system_message() {
        let fixture = create_request_with_system_messages(1);
        let mut transformer = AuthSystemMessage.when(|_| true);

        let actual = transformer.transform(fixture);

        let system_messages = actual.system.unwrap();
        assert_eq!(system_messages.len(), 2);
        assert_eq!(system_messages[0].text, AUTH_MESSAGE.trim());
        assert_eq!(system_messages[1].text, "System message 0");
    }

    #[test]
    fn test_disabled_preserves_existing_system_messages() {
        let fixture = create_request_with_system_messages(2);
        let expected_count = fixture.system.as_ref().unwrap().len();

        let mut transformer = AuthSystemMessage.when(|_| false);
        let actual = transformer.transform(fixture);

        let system_messages = actual.system.unwrap();
        assert_eq!(system_messages.len(), expected_count);
        assert_eq!(system_messages[0].text, "System message 0");
        assert_eq!(system_messages[1].text, "System message 1");
    }
}
