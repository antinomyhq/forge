use super::super::SummaryMessage;
use crate::compact::summary::ContextSummary;
use crate::{Role, Transformer};

/// Keeps only the first user message in consecutive user message sequences.
///
/// This transformer processes a context summary and filters out consecutive
/// user messages, keeping only the first one in each sequence. Messages with
/// other roles (System, Assistant) are preserved as-is.
pub struct KeepFirstUserMessage;

impl Transformer for KeepFirstUserMessage {
    type Value = ContextSummary;

    fn transform(&mut self, summary: Self::Value) -> Self::Value {
        let mut messages: Vec<SummaryMessage> = Vec::new();
        let mut last_role = Role::System;
        for message in summary.messages {
            let role = message.role;
            if role == Role::User {
                if last_role != Role::User {
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
    use crate::compact::summary::{SummaryMessage, SummaryMessageBlock, SummaryToolCall};

    fn fixture_message_block() -> SummaryMessageBlock {
        SummaryMessageBlock {
            content: Some("test content".to_string()),
            tool_call_id: None,
            tool_call: SummaryToolCall::Execute { cmd: "test".to_string() },
            tool_call_success: None,
        }
    }

    fn fixture_summary_message(role: Role) -> SummaryMessage {
        SummaryMessage { role, messages: vec![fixture_message_block()] }
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

        let mut transformer = KeepFirstUserMessage;
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

        let mut transformer = KeepFirstUserMessage;
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

        let mut transformer = KeepFirstUserMessage;
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

        let mut transformer = KeepFirstUserMessage;
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

        let mut transformer = KeepFirstUserMessage;
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
}
