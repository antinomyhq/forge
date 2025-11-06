use forge_domain::{ContextSummary, Role, Transformer};

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
    use forge_domain::{SummaryBlock, SummaryMessage as Block, SummaryToolCall};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_empty_summary() {
        let fixture = ContextSummary::new(vec![]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = ContextSummary::new(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_system_role() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::System, vec![Block::content("System prompt")]),
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
            SummaryBlock::new(Role::Assistant, vec![Block::content("Assistant response")]),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
            SummaryBlock::new(Role::Assistant, vec![Block::content("Assistant response")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_user_role() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::System, vec![Block::content("System prompt")]),
            SummaryBlock::new(Role::User, vec![Block::content("User message 1")]),
            SummaryBlock::new(Role::Assistant, vec![Block::content("Assistant response")]),
            SummaryBlock::new(Role::User, vec![Block::content("User message 2")]),
        ]);
        let actual = DropRole::new(Role::User).transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryBlock::new(Role::System, vec![Block::content("System prompt")]),
            SummaryBlock::new(Role::Assistant, vec![Block::content("Assistant response")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_assistant_role() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
            SummaryBlock::new(
                Role::Assistant,
                vec![Block::content("Assistant response 1")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![Block::content("Assistant response 2")],
            ),
        ]);
        let actual = DropRole::new(Role::Assistant).transform(fixture);

        let expected = ContextSummary::new(vec![SummaryBlock::new(
            Role::User,
            vec![Block::content("User message")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_drops_multiple_messages_of_same_role() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::System, vec![Block::content("First system message")]),
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
            SummaryBlock::new(Role::System, vec![Block::content("Second system message")]),
            SummaryBlock::new(Role::Assistant, vec![Block::content("Assistant response")]),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
            SummaryBlock::new(Role::Assistant, vec![Block::content("Assistant response")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_other_roles() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::User, vec![Block::content("User message 1")]),
            SummaryBlock::new(
                Role::Assistant,
                vec![Block::content("Assistant response 1")],
            ),
            SummaryBlock::new(Role::User, vec![Block::content("User message 2")]),
            SummaryBlock::new(
                Role::Assistant,
                vec![Block::content("Assistant response 2")],
            ),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryBlock::new(Role::User, vec![Block::content("User message 1")]),
            SummaryBlock::new(
                Role::Assistant,
                vec![Block::content("Assistant response 1")],
            ),
            SummaryBlock::new(Role::User, vec![Block::content("User message 2")]),
            SummaryBlock::new(
                Role::Assistant,
                vec![Block::content("Assistant response 2")],
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_target_role_results_in_empty() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::System, vec![Block::content("System message 1")]),
            SummaryBlock::new(Role::System, vec![Block::content("System message 2")]),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = ContextSummary::new(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_tool_calls_in_non_dropped_messages() {
        let fixture = ContextSummary::new(vec![
            SummaryBlock::new(Role::System, vec![Block::content("System with tool")]),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::read("/src/main.rs").into(),
                    SummaryToolCall::update("/src/lib.rs").into(),
                ],
            ),
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
        ]);
        let actual = DropRole::new(Role::System).transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::read("/src/main.rs").into(),
                    SummaryToolCall::update("/src/lib.rs").into(),
                ],
            ),
            SummaryBlock::new(Role::User, vec![Block::content("User message")]),
        ]);

        assert_eq!(actual, expected);
    }
}
