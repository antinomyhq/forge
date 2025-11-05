use derive_setters::Setters;
use indexmap::IndexMap;

use crate::compact::summary::{ContextSummary, SummaryMessageBlock, SummaryToolCall};
use crate::{Role, Transformer};

/// Trims context summary by keeping only the last operation for each path.
///
/// This transformer deduplicates file operations within assistant messages by
/// retaining only the most recent operation for each file path. Only applies
/// to messages with the Assistant role. This is useful for reducing context
/// size while preserving the final state of file operations.
pub struct TrimContextSummary;

#[derive(Default, Hash, PartialEq, Eq, Setters)]
struct BlockKey {
    path: Option<String>,
    content: Option<String>,
    tool_call_id: Option<String>,
}

impl From<&SummaryMessageBlock> for BlockKey {
    fn from(value: &SummaryMessageBlock) -> Self {
        match value {
            SummaryMessageBlock::Content(text) => {
                Self { path: None, content: Some(text.clone()), tool_call_id: None }
            }
            SummaryMessageBlock::ToolCall(tool_data) => Self {
                path: extract_path(&tool_data.tool_call),
                content: None,
                tool_call_id: tool_data.tool_call_id.as_ref().map(|id| id.0.clone()),
            },
        }
    }
}

fn extract_path(tool_call: &SummaryToolCall) -> Option<String> {
    match tool_call {
        SummaryToolCall::FileRead { path } => Some(path.to_owned()),
        SummaryToolCall::FileUpdate { path } => Some(path.to_owned()),
        SummaryToolCall::FileRemove { path } => Some(path.to_owned()),
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

            let mut block_map: IndexMap<BlockKey, SummaryMessageBlock> = IndexMap::new();

            for block in message.blocks.drain(..) {
                // For tool calls, only keep successful operations
                if let SummaryMessageBlock::ToolCall(ref tool_data) = block {
                    if !tool_data.tool_call_success {
                        continue;
                    }

                    let path = extract_path(&tool_data.tool_call);
                    let key = BlockKey::default().path(path);

                    // Remove previous entry if it has no content
                    if let Some(SummaryMessageBlock::ToolCall(prev_tool_data)) = block_map.get(&key)
                        && prev_tool_data.tool_call_id.is_none()
                    {
                        block_map.shift_remove(&key);
                    }
                }

                block_map.insert(BlockKey::from(&block), block);
            }

            message.blocks = block_map.into_values().collect();
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
            tool_call: call,
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
                    tool_call: SummaryToolCall::FileRead { path: "/unknown".to_string() },
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
}
