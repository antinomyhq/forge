use std::collections::HashMap;

use crate::compact::summary::{ContextSummary, SummaryMessageBlock, SummaryToolCall};
use crate::{Role, Transformer};

/// Trims context summary by keeping only the last operation for each path.
///
/// This transformer deduplicates file operations within assistant messages by
/// retaining only the most recent operation for each file path. Only applies
/// to messages with the Assistant role. This is useful for reducing context
/// size while preserving the final state of file operations.
pub struct TrimContextSummary;

impl TrimContextSummary {
    /// Extracts the path from a tool call, if available
    fn extract_path(tool_call: &SummaryToolCall) -> Option<&str> {
        match tool_call {
            SummaryToolCall::FileRead { path } => Some(path.as_str()),
            SummaryToolCall::FileUpdate { path } => Some(path.as_str()),
            SummaryToolCall::FileRemove { path } => Some(path.as_str()),
        }
    }
}

impl Transformer for TrimContextSummary {
    type Value = ContextSummary;

    fn transform(&mut self, mut summary: Self::Value) -> Self::Value {
        for message in summary.messages.iter_mut() {
            // Only apply trimming to Assistant role messages
            if message.role != Role::Assistant {
                continue;
            }

            // Map to track the last successful operation for each file path
            let mut last_operations: HashMap<String, SummaryMessageBlock> = HashMap::new();
            let mut insertion_order: Vec<String> = Vec::new();

            for block in message.messages.drain(..) {
                // Only keep successful operations
                if block.tool_call_success != Some(true) {
                    continue;
                }

                if let Some(path) = Self::extract_path(&block.tool_call) {
                    let key = path.to_string();

                    // Track insertion order only for new keys
                    if !last_operations.contains_key(&key) {
                        insertion_order.push(key.clone());
                    }
                    // Store or update the last operation for this path
                    last_operations.insert(key, block);
                }
            }

            // Reconstruct messages in original insertion order
            message.messages = insertion_order
                .into_iter()
                .filter_map(|key| last_operations.remove(&key))
                .collect();
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
                        fixture_message_block(SummaryToolCall::FileUpdate {
                            path: "file.txt".to_string(),
                        }),
                        fixture_message_block(SummaryToolCall::FileUpdate {
                            path: "file.txt".to_string(),
                        }),
                    ],
                },
            ],
        };

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        // Assistant messages should be trimmed
        assert_eq!(actual.messages[0].messages.len(), 1);
        // User messages should NOT be trimmed
        assert_eq!(actual.messages[1].messages.len(), 2);
    }

    #[test]
    fn test_handles_empty_summary() {
        let fixture = ContextSummary { messages: vec![] };

        let mut transformer = TrimContextSummary;
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

        let mut transformer = TrimContextSummary;
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

        let mut transformer = TrimContextSummary;
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

        let mut transformer = TrimContextSummary;
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

        let mut transformer = TrimContextSummary;
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

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_filters_out_failed_operations() {
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

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        // Only the successful operation should remain
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(actual.messages[0].messages[0].tool_call_success, Some(true));
    }

    #[test]
    fn test_keeps_last_operation_regardless_of_content() {
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

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        // Only the last operation for the path should remain
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(
            actual.messages[0].messages[0].content,
            Some("content2".to_string())
        );
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
                        tool_call: SummaryToolCall::FileUpdate { path: "file.txt".to_string() },
                        tool_call_success: Some(true),
                    },
                    SummaryMessageBlock {
                        content: None,
                        tool_call_id: None,
                        tool_call: SummaryToolCall::FileUpdate { path: "file.txt".to_string() },
                        tool_call_success: Some(true),
                    },
                ],
            }],
        };

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_does_not_trim_user_role_messages() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::User,
                messages: vec![
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test".to_string() }),
                    fixture_message_block(SummaryToolCall::FileRead { path: "/test".to_string() }),
                ],
            }],
        };

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        // User messages should not be trimmed, all 3 blocks should remain
        assert_eq!(actual.messages[0].messages.len(), 3);
    }

    #[test]
    fn test_does_not_trim_system_role_messages() {
        let fixture = ContextSummary {
            messages: vec![SummaryMessage {
                role: Role::System,
                messages: vec![
                    fixture_message_block(SummaryToolCall::FileUpdate {
                        path: "file.txt".to_string(),
                    }),
                    fixture_message_block(SummaryToolCall::FileUpdate {
                        path: "file.txt".to_string(),
                    }),
                ],
            }],
        };

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        // System messages should not be trimmed, both blocks should remain
        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_only_trims_assistant_messages_in_mixed_roles() {
        let fixture = ContextSummary {
            messages: vec![
                SummaryMessage {
                    role: Role::User,
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
                    role: Role::Assistant,
                    messages: vec![
                        fixture_message_block(SummaryToolCall::FileUpdate {
                            path: "file.txt".to_string(),
                        }),
                        fixture_message_block(SummaryToolCall::FileUpdate {
                            path: "file.txt".to_string(),
                        }),
                    ],
                },
                SummaryMessage {
                    role: Role::System,
                    messages: vec![
                        fixture_message_block(SummaryToolCall::FileRemove {
                            path: "remove.txt".to_string(),
                        }),
                        fixture_message_block(SummaryToolCall::FileRemove {
                            path: "remove.txt".to_string(),
                        }),
                    ],
                },
            ],
        };

        let mut transformer = TrimContextSummary;
        let actual = transformer.transform(fixture);

        // User messages should not be trimmed
        assert_eq!(actual.messages[0].messages.len(), 2);
        // Assistant messages should be trimmed
        assert_eq!(actual.messages[1].messages.len(), 1);
        // System messages should not be trimmed
        assert_eq!(actual.messages[2].messages.len(), 2);
    }
}
