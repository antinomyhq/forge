use forge_app::domain::Transformer;

use crate::openai::request::{Request, Role};

/// O1 Models do not support System Roles. Only User || Assistant
pub struct RoleChoice;

impl Transformer for RoleChoice {
    type Value = Request;

    /// Since the models do not support System Role. We convert the Role to User
    /// instead
    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        if let Some(messages) = request.messages.as_mut() {
            for message in messages.iter_mut() {
                if message.role == Role::System {
                    message.role = Role::User;
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
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::openai::request::{Message, MessageContent};

    fn create_message_fixture(role: Role, content: &str) -> Message {
        Message {
            role,
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_details: None,
        }
    }

    fn create_request_fixture(messages: Vec<Message>) -> Request {
        Request { messages: Some(messages), ..Default::default() }
    }

    fn assert_messages_roles_eq(actual_messages: &Option<Vec<Message>>, expected_roles: &[Role]) {
        match actual_messages {
            Some(messages) => {
                assert_eq!(messages.len(), expected_roles.len());
                for (message, expected_role) in messages.iter().zip(expected_roles.iter()) {
                    assert_eq!(message.role, *expected_role);
                }
            }
            None => assert_eq!(expected_roles.len(), 0),
        }
    }

    #[test]
    fn test_transform_system_role_to_user() {
        let fixture = create_request_fixture(vec![create_message_fixture(
            Role::System,
            "You are a helpful assistant",
        )]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(&actual.messages, &[Role::User]);
    }

    #[test]
    fn test_preserve_user_role() {
        let fixture = create_request_fixture(vec![create_message_fixture(
            Role::User,
            "Hello, how are you?",
        )]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(&actual.messages, &[Role::User]);
    }

    #[test]
    fn test_preserve_assistant_role() {
        let fixture = create_request_fixture(vec![create_message_fixture(
            Role::Assistant,
            "I'm doing well, thank you!",
        )]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(&actual.messages, &[Role::Assistant]);
    }

    #[test]
    fn test_preserve_tool_role() {
        let fixture =
            create_request_fixture(vec![create_message_fixture(Role::Tool, "Tool response")]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(&actual.messages, &[Role::Tool]);
    }

    #[test]
    fn test_transform_mixed_messages() {
        let fixture = create_request_fixture(vec![
            create_message_fixture(Role::System, "You are a helpful assistant"),
            create_message_fixture(Role::User, "Hello"),
            create_message_fixture(Role::Assistant, "Hi there!"),
            create_message_fixture(Role::System, "Be concise"),
            create_message_fixture(Role::Tool, "Tool result"),
        ]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(
            &actual.messages,
            &[
                Role::User,      // System -> User
                Role::User,      // User stays User
                Role::Assistant, // Assistant stays Assistant
                Role::User,      // System -> User
                Role::Tool,      // Tool stays Tool
            ],
        );
    }

    #[test]
    fn test_transform_empty_messages() {
        let fixture = create_request_fixture(vec![]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(&actual.messages, &[]);
    }

    #[test]
    fn test_transform_none_messages() {
        let fixture = Request { messages: None, ..Default::default() };
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert!(actual.messages.is_none());
    }

    #[test]
    fn test_transform_only_system_messages() {
        let fixture = create_request_fixture(vec![
            create_message_fixture(Role::System, "First system message"),
            create_message_fixture(Role::System, "Second system message"),
        ]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        assert_messages_roles_eq(&actual.messages, &[Role::User, Role::User]);
    }

    #[test]
    fn test_transform_preserves_message_content() {
        let fixture = create_request_fixture(vec![create_message_fixture(
            Role::System,
            "You are a helpful assistant",
        )]);
        let mut transformer = RoleChoice;

        let actual = transformer.transform(fixture);

        let messages = actual.messages.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::User);
        if let Some(MessageContent::Text(content)) = &messages[0].content {
            assert_eq!(content, "You are a helpful assistant");
        } else {
            panic!("Expected text content");
        }
    }
}
