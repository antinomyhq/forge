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
        for message in summary.messages {
            let role = message.role;
            if role == self.role {
                if last_role != self.role {
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
    use crate::compact::summary::{
        SummaryMessage, SummaryMessageBlock, SummaryToolCall, SummaryToolData,
    };

    fn fixture_message_block() -> SummaryMessageBlock {
        SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: None,
            tool_call: SummaryToolCall::FileRead { path: "test".to_string() },
            tool_call_success: false,
        })
    }

    fn fixture_summary_message(role: Role) -> SummaryMessage {
        SummaryMessage { role, blocks: vec![fixture_message_block()] }
    }

    #[test]
    fn test_keeps_first_user_message_in_sequence() {
        let fixture = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::User),
            ],
        };

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary { messages: vec![fixture_summary_message(Role::User)] };

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_preserves_non_user_messages() {
        let fixture = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::System),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::System),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_keeps_first_user_message_per_sequence() {
        let fixture = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::User),
            ],
        };

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_handles_empty_messages() {
        let fixture = ContextSummary { messages: vec![] };

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary { messages: vec![] };

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_handles_mixed_roles() {
        let fixture = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::System),
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        let mut transformer = DedupeRole::new(Role::User);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::System),
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        assert_eq!(actual.messages.len(), expected.messages.len());
    }

    #[test]
    fn test_dedupes_assistant_role() {
        let fixture = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        let mut transformer = DedupeRole::new(Role::Assistant);
        let actual = transformer.transform(fixture);

        let expected = ContextSummary {
            messages: vec![
                fixture_summary_message(Role::User),
                fixture_summary_message(Role::Assistant),
                fixture_summary_message(Role::User),
            ],
        };

        assert_eq!(actual.messages.len(), expected.messages.len());
    }
}
