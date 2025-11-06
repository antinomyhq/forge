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
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::compact::summary::SummaryMessage;
    use crate::{Role, ToolCallId};

    // Alias for convenience in tests
    type Block = SummaryMessageBlock;

    #[test]
    fn test_empty_summary() {
        let fixture = ContextSummary::new(vec![]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_keeps_last_operation_per_path() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(None, "/test1"),
                Block::read(None, "/test2"),
                Block::read(None, "/test2"),
                Block::read(None, "/test3"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
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
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(Some(ToolCallId::new("call1")), "/test"),
                Block::read(Some(ToolCallId::new("call2")), "/test"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![Block::read(Some(ToolCallId::new("call2")), "/test")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_different_operation_types_on_same_path() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
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

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(None, "/test"),
                Block::update(None, "file.txt"),
                Block::remove(None, "/test"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filters_failed_and_none_operations() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read_with_status(None, "/test", true),
                Block::read_with_status(None, "/test", false),
                Block::read_with_status(None, "/test", true),
                Block::read(None, "/unknown"),
                Block::read_with_status(None, "/unknown", false),
                Block::update(None, "file.txt"),
                Block::read_with_status(None, "/all_failed", false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read_with_status(None, "/test", true),
                Block::read(None, "/unknown"),
                Block::update(None, "file.txt"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_assistant_message_after_filtering() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read_with_status(None, "/test1", false),
                Block::read_with_status(None, "/test2", false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(Role::Assistant, vec![])]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_trims_assistant_messages() {
        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(
                Role::User,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
            SummaryMessage::new(
                Role::Assistant,
                vec![
                    Block::update(None, "file.txt"),
                    Block::update(None, "file.txt"),
                ],
            ),
            SummaryMessage::new(
                Role::System,
                vec![
                    Block::remove(None, "remove.txt"),
                    Block::remove(None, "remove.txt"),
                ],
            ),
            SummaryMessage::new(
                Role::Assistant,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(
                Role::User,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
            SummaryMessage::new(Role::Assistant, vec![Block::update(None, "file.txt")]),
            SummaryMessage::new(
                Role::System,
                vec![
                    Block::remove(None, "remove.txt"),
                    Block::remove(None, "remove.txt"),
                ],
            ),
            SummaryMessage::new(Role::Assistant, vec![Block::read(None, "/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_assistant_messages_trimmed_independently() {
        let fixture = ContextSummary::new(vec![
            SummaryMessage::new(
                Role::Assistant,
                vec![Block::read(None, "/test"), Block::read(None, "/test")],
            ),
            SummaryMessage::new(
                Role::Assistant,
                vec![Block::read_with_status(None, "/test", false)],
            ),
            SummaryMessage::new(
                Role::Assistant,
                vec![
                    Block::read(None, "/test"),
                    Block::read(None, "/test"),
                    Block::read(None, "/test"),
                ],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::Assistant, vec![Block::read(None, "/test")]),
            SummaryMessage::new(Role::Assistant, vec![]),
            SummaryMessage::new(Role::Assistant, vec![Block::read(None, "/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_assistant_message_with_different_call_ids() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("foo"),
                Block::read(Some(ToolCallId::new("1")), "/test1"),
                Block::read(Some(ToolCallId::new("2")), "/test1"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("foo"),
                Block::read(Some(ToolCallId::new("2")), "/test1"),
            ],
        )]);

        assert_eq!(actual, expected);
    }
}
