use crate::compact::summary::{ContextSummary, SummaryMessageBlock};
use crate::{CanMerge, Transformer};

/// Merges all messages within a context summary.
///
/// This transformer consolidates consecutive mergeable message blocks within
/// each message in the context summary. Adjacent `SummaryMessageBlock`
/// instances that are mergeable according to the `CanMerge` trait
/// implementation are consolidated.
pub struct MergeContextSummary;

impl Transformer for MergeContextSummary {
    type Value = ContextSummary;

    fn transform(&mut self, mut summary: Self::Value) -> Self::Value {
        for message in summary.messages.iter_mut() {
            let mut merged_blocks: Vec<SummaryMessageBlock> = Vec::new();

            for block in message.messages.drain(..) {
                if let Some(last) = merged_blocks.last_mut()
                    && last.can_merge(&block)
                {
                    *last = block;
                } else {
                    merged_blocks.push(block);
                }
            }

            message.messages = merged_blocks;
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Role;
    use crate::compact::summary::{SummaryMessage, SummaryMessageBlock, SummaryToolCall};

    fn fixture_message_block(tool_call: SummaryToolCall) -> SummaryMessageBlock {
        SummaryMessageBlock {
            content: Some("test".to_string()),
            tool_call_id: None,
            tool_call,
            tool_call_success: Some(true),
        }
    }

    #[test]
    fn test_merges_all_messages_in_summary() {
        let fixture = ContextSummary {
            messages: vec![
                SummaryMessage {
                    role: Role::Assistant,
                    messages: vec![
                        fixture_message_block(SummaryToolCall::FileRead {
                            path: "/test".to_string(),
                        }),
                        fixture_message_block(SummaryToolCall::FileRead {
                            path: "/test".to_string(),
                        }),
                    ],
                },
                SummaryMessage {
                    role: Role::User,
                    messages: vec![
                        fixture_message_block(SummaryToolCall::Execute { cmd: "ls".to_string() }),
                        fixture_message_block(SummaryToolCall::Execute { cmd: "ls".to_string() }),
                    ],
                },
            ],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(actual.messages[1].messages.len(), 1);
    }

    #[test]
    fn test_handles_empty_summary() {
        let fixture = ContextSummary { messages: vec![] };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages.len(), 0);
    }

    #[test]
    fn test_handles_messages_with_no_mergeable_blocks() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test1".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test2".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test3".to_string() }),
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 3);
    }

    #[test]
    fn test_preserves_message_roles() {
        let fixture = ContextSummary {
            messages: vec![
                SummaryMessage { role: Role::System, messages: vec![] },
                SummaryMessage { role: Role::User, messages: vec![] },
                SummaryMessage { role: Role::Assistant, messages: vec![] },
            ],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].role, Role::System);
        assert_eq!(actual.messages[1].role, Role::User);
        assert_eq!(actual.messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_handles_mixed_mergeable_and_non_mergeable() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test1".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test2".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test2".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test3".to_string() }),
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 3);
    }

    #[test]
    fn test_merges_consecutive_identical_blocks() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 1);
    }

    #[test]
    fn test_does_not_merge_different_tool_calls() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test1".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test2".to_string() },
                        tool_call_success: Some(true),
                    },
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_does_not_merge_different_success_status() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: Some("content".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(false),
                    },
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_does_not_merge_different_content() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    SummaryMessageBlock {
                        content: Some("content1".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: Some("content2".to_string()),
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_merges_different_tool_types_correctly() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::Assistant,
                messages: vec![
                    SummaryMessageBlock {
                        content: None,
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: None,
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: None,
                        tool_call_id: None,
                        tool_call: SummaryToolCall::Execute { cmd: "ls".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: None,
                        tool_call_id: None,
                        tool_call: SummaryToolCall::Execute { cmd: "ls".to_string() },
                        tool_call_success: Some(true),
                    },
                ],
            }],
        };

        let mut transformer = MergeContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 2);
    }
}
