use super::super::SummaryMessage;
use crate::compact::summary::ContextSummary;
use crate::{Role, Transformer};

/// Keeps only the first message in consecutive sequences of a specific role.
///
/// This transformer processes a context summary and filters out consecutive
/// messages of the specified role, keeping only the first one in each sequence.
/// Messages with other roles are preserved as-is.
pub struct DedupeRole {
    role: Role,
}

impl DedupeRole {
    /// Creates a new DedupeConsecutiveRole transformer for the specified role.
    ///
    /// # Arguments
    ///
    /// * `role` - The role to deduplicate in consecutive sequences
    pub fn new(role: Role) -> Self {
        Self { role }
    }
}

impl Transformer for DedupeRole {
    type Value = ContextSummary;

    fn transform(&mut self, summary: Self::Value) -> Self::Value {
        let mut messages: Vec<SummaryMessage> = Vec::new();
        let mut last_role = Role::System;
        for mut message in summary.messages {
            let role = message.role;
            if role == self.role {
                if last_role != self.role {
                    message.blocks.drain(1..);
                    messages.push(message)
                }
            } else {
                messages.push(message)
            }

            last_role = role;
        }

        ContextSummary { messages }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::compact::summary::{SummaryMessage, SummaryMessageBlock as Block};

    #[test]
    fn test_keeps_first_user_message_in_sequence() {
        let block = Block::read_with_status(None, "test", false);
        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
        ]);

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(Role::User, vec![block])]);

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_preserves_non_user_messages() {
        let block = Block::read_with_status(None, "test", false);
        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(Role::System, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
        ]);

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::System, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block]),
        ]);

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_keeps_first_user_message_per_sequence() {
        let block = Block::read_with_status(None, "test", false);

        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
        ]);

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block]),
        ]);

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_handles_empty_messages() {
        let fixture = ContextSummary::new(vec![]);

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![]);

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_handles_mixed_roles() {
        let block = Block::read_with_status(None, "test", false);

        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(Role::System, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
        ]);

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::System, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block]),
        ]);

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_dedupes_assistant_role() {
        let block = Block::read_with_status(None, "test", false);

        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block.clone()]),
        ]);

        let mut transformer = DedupeRole::new(Role::Assistant);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone()]),
            SummaryMessage::new(Role::User, vec![block]),
        ]);

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_drains_all_blocks_except_first_in_deduplicated_messages() {
        let block = Block::read_with_status(None, "test", false);

        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(
                Role::User,
                vec![block.clone(), block.clone(), block.clone()],
            ),
            SummaryMessage::new(Role::User, vec![block.clone(), block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone(), block.clone()]),
            SummaryMessage::new(
                Role::User,
                vec![block.clone(), block.clone(), block.clone(), block.clone()],
            ),
        ]);

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![block.clone()]),
            SummaryMessage::new(Role::Assistant, vec![block.clone(), block.clone()]),
            SummaryMessage::new(Role::User, vec![block]),
        ]);

        assert_eq!(actual, expected);
    }
}
