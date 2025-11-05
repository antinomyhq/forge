use crate::compact::summary::{ContextSummary, SummaryMessageBlock, SummaryToolCall};
use crate::{Role, Transformer};

/// Trims context summary by keeping only the last operation for each path.
///
/// This transformer deduplicates file operations within assistant messages by
/// retaining only the most recent operation for each file path. Only applies
/// to messages with the Assistant role. This is useful for reducing context
/// size while preserving the final state of file operations.
pub struct TrimContextSummary;

fn extract_path(tool_call: &SummaryToolCall) -> &str {
    match tool_call {
        SummaryToolCall::FileRead { path } => path.as_str(),
        SummaryToolCall::FileUpdate { path } => path.as_str(),
        SummaryToolCall::FileRemove { path } => path.as_str(),
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

            let mut block_seq: Vec<SummaryMessageBlock> = Default::default();

            for block in message.blocks.drain(..) {
                // For tool calls, only keep successful operations
                if let SummaryMessageBlock::ToolCall(ref current) = block {
                    if !current.tool_call_success {
                        continue;
                    }

                    // Remove previous entry if it has no content
                    if let Some(SummaryMessageBlock::ToolCall(last_tool_data)) =
                        block_seq.last_mut()
                    {
                        let last_path = extract_path(&last_tool_data.call);
                        let path = extract_path(&current.call);
                        if last_path == path {
                            block_seq.pop();
                        }
                    }
                }

                block_seq.push(block);
            }

            message.blocks = block_seq;
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    // Alias for convenience in tests
    use SummaryMessageBlock as Block;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::compact::summary::{
        SummaryMessage, SummaryMessageBlock, SummaryToolCall, SummaryToolData,
    };
    use crate::{Role, ToolCallId};

    // Helper to create a summary message with role and blocks
    fn message(role: Role, blocks: Vec<SummaryMessageBlock>) -> SummaryMessage {
        SummaryMessage { role, blocks }
    }

    // Helper to create a context summary
    fn summary(messages: Vec<SummaryMessage>) -> ContextSummary {
        ContextSummary { messages }
    }

    // Helper to create a successful tool call block
    fn tool_block(call: SummaryToolCall, success: bool) -> SummaryMessageBlock {
        SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: None,
            call,
            tool_call_success: success,
        })
    }

    #[test]
    fn test_empty_summary() {
        let fixture = summary(vec![]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_keeps_last_operation_per_path() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/test1"),
                Block::read(None, "/test2"),
                Block::read(None, "/test2"),
                Block::read(None, "/test3"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/test1"),
                Block::read(None, "/test2"),
                Block::read(None, "/test3"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_keeps_last_operation_with_content() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(Some(ToolCallId::new("call1")), "/test"),
                Block::read(Some(ToolCallId::new("call2")), "/test"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(Some(ToolCallId::new("call1")), "/test"),
                Block::read(Some(ToolCallId::new("call2")), "/test"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_different_operation_types_on_same_path() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/test"),
                Block::read(None, "/test"),
                Block::update(None, "file.txt"),
                Block::update(None, "file.txt"),
                Block::read(None, "/test"),
                Block::update(None, "/test"),
                Block::remove(None, "/test"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::update(None, "file.txt"),
                Block::remove(None, "/test"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_insertion_order() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/path1"),
                Block::read(None, "/path2"),
                Block::read(None, "/path1"),
                Block::update(None, "/path3"),
                Block::read(None, "/path2"),
                Block::remove(None, "/path1"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::update(None, "/path3"),
                Block::read(None, "/path2"),
                Block::remove(None, "/path1"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filters_failed_and_none_operations() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                tool_block(
                    SummaryToolCall::FileRead { path: "/test".to_string() },
                    true,
                ),
                tool_block(
                    SummaryToolCall::FileRead { path: "/test".to_string() },
                    false,
                ),
                tool_block(
                    SummaryToolCall::FileRead { path: "/test".to_string() },
                    true,
                ),
                Block::read(None, "/unknown"),
                SummaryMessageBlock::ToolCall(SummaryToolData {
                    tool_call_id: None,
                    call: SummaryToolCall::FileRead { path: "/unknown".to_string() },
                    tool_call_success: false, // Should be filtered out
                }),
                Block::update(None, "file.txt"),
                tool_block(
                    SummaryToolCall::FileRead { path: "/all_failed".to_string() },
                    false,
                ),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                tool_block(
                    SummaryToolCall::FileRead { path: "/test".to_string() },
                    true,
                ),
                Block::read(None, "/unknown"),
                Block::update(None, "file.txt"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_assistant_message_after_filtering() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                tool_block(
                    SummaryToolCall::FileRead { path: "/test1".to_string() },
                    false,
                ),
                tool_block(
                    SummaryToolCall::FileRead { path: "/test2".to_string() },
                    false,
                ),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(Role::Assistant, vec![])]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_trims_assistant_messages() {
        let fixture = summary(vec![
            message(
                Role::User,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::update(None, "file.txt"),
                    Block::update(None, "file.txt"),
                ],
            ),
            message(
                Role::System,
                vec![
                    Block::remove(None, "remove.txt"),
                    Block::remove(None, "remove.txt"),
                ],
            ),
            message(
                Role::Assistant,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![
            message(
                Role::User,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
            message(Role::Assistant, vec![Block::update(None, "file.txt")]),
            message(
                Role::System,
                vec![
                    Block::remove(None, "remove.txt"),
                    Block::remove(None, "remove.txt"),
                ],
            ),
            message(Role::Assistant, vec![Block::read(None, "/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_assistant_messages_trimmed_independently() {
        let fixture = summary(vec![
            message(
                Role::Assistant,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
            message(
                Role::Assistant,
                vec![tool_block(
                    SummaryToolCall::FileRead { path: "/test".to_string() },
                    false,
                )],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::read(None, "/test"),
                    Block::read(None, "/test"),
                    Block::read(None, "/test"),
                ],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![
            message(Role::Assistant, vec![Block::read(None, "/test")]),
            message(Role::Assistant, vec![]),
            message(Role::Assistant, vec![Block::read(None, "/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_assistant_message_with_content() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read(None, "/test1"),
                Block::update(None, "/test2"),
                Block::content("foo"),
                Block::read(None, "/test1"),
                Block::read(Some(ToolCallId::new("call1")), "/test2"),
                Block::content("baz"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::content("foo"),
                Block::read(None, "/test1"),
                Block::read(Some(ToolCallId::new("call1")), "/test2"),
                Block::content("baz"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_assistant_message_with_different_call_ids() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::content("foo"),
                Block::read(Some(ToolCallId::new("1")), "/test1"),
                Block::read(Some(ToolCallId::new("2")), "/test1"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::content("foo"),
                Block::read(Some(ToolCallId::new("2")), "/test1"),
            ],
        )]);

        assert_eq!(actual, expected);
    }
}
