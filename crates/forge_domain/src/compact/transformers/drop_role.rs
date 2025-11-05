use crate::compact::summary::ContextSummary;
use crate::{Role, Transformer};

/// Drops all messages with a specific role from the context summary.
///
/// This transformer removes all messages matching the specified role, which is
/// useful for reducing context size when certain message types are not needed
/// in summaries. For example, system messages containing initial prompts and
/// instructions often don't need to be preserved in compacted contexts.
pub struct DropRole {
    role: Role,
}

impl DropRole {
    /// Creates a new DropRole transformer for the specified role.
    ///
    /// # Arguments
    ///
    /// * `role` - The role to drop from the context summary
    pub fn new(role: Role) -> Self {
        Self { role }
    }
}

impl Transformer for DropRole {
    type Value = ContextSummary;

    fn transform(&mut self, mut summary: Self::Value) -> Self::Value {
        summary.messages.retain(|msg| msg.role != self.role);
        summary
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::compact::summary::{SummaryMessage, SummaryMessageBlock as Block};

    // Helper to create a summary message with role and blocks
    fn message(role: Role, blocks: Vec<Block>) -> SummaryMessage {
        SummaryMessage { role, messages: blocks }
    }

    // Helper to create a context summary
    fn summary(messages: Vec<SummaryMessage>) -> ContextSummary {
        ContextSummary { messages }
    }

    #[test]
    fn test_empty_summary() {
        let fixture = summary(vec![]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = summary(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_system_role() {
        let fixture = summary(vec![
            message(
                Role::System,
                vec![Block::default().content("System prompt")],
            ),
            message(Role::User, vec![Block::default().content("User message")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response")],
            ),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![Block::default().content("User message")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response")],
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_user_role() {
        let fixture = summary(vec![
            message(
                Role::System,
                vec![Block::default().content("System prompt")],
            ),
            message(Role::User, vec![Block::default().content("User message 1")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response")],
            ),
            message(Role::User, vec![Block::default().content("User message 2")]),
        ]);
        let actual = DropRole::new(Role::User).transform(fixture);

        let expected = summary(vec![
            message(
                Role::System,
                vec![Block::default().content("System prompt")],
            ),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response")],
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_assistant_role() {
        let fixture = summary(vec![
            message(Role::User, vec![Block::default().content("User message")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response 1")],
            ),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response 2")],
            ),
        ]);
        let actual = DropRole::new(Role::Assistant).transform(fixture);

        let expected = summary(vec![message(
            Role::User,
            vec![Block::default().content("User message")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_multiple_messages_of_same_role() {
        let fixture = summary(vec![
            message(
                Role::System,
                vec![Block::default().content("First system message")],
            ),
            message(Role::User, vec![Block::default().content("User message")]),
            message(
                Role::System,
                vec![Block::default().content("Second system message")],
            ),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response")],
            ),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![Block::default().content("User message")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response")],
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_other_roles() {
        let fixture = summary(vec![
            message(Role::User, vec![Block::default().content("User message 1")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response 1")],
            ),
            message(Role::User, vec![Block::default().content("User message 2")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response 2")],
            ),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![Block::default().content("User message 1")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response 1")],
            ),
            message(Role::User, vec![Block::default().content("User message 2")]),
            message(
                Role::Assistant,
                vec![Block::default().content("Assistant response 2")],
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_target_role_results_in_empty() {
        let fixture = summary(vec![
            message(
                Role::System,
                vec![Block::default().content("System message 1")],
            ),
            message(
                Role::System,
                vec![Block::default().content("System message 2")],
            ),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = summary(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_tool_calls_in_non_dropped_messages() {
        let fixture = summary(vec![
            message(
                Role::System,
                vec![Block::default().content("System with tool")],
            ),
            message(
                Role::Assistant,
                vec![Block::read("/src/main.rs"), Block::update("/src/lib.rs")],
            ),
            message(Role::User, vec![Block::default().content("User message")]),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = summary(vec![
            message(
                Role::Assistant,
                vec![Block::read("/src/main.rs"), Block::update("/src/lib.rs")],
            ),
            message(Role::User, vec![Block::default().content("User message")]),
        ]);

        assert_eq!(actual, expected);
    }
}
