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
}

impl From<&SummaryMessageBlock> for BlockKey {
    fn from(value: &SummaryMessageBlock) -> Self {
        Self { path: extract_path(value), content: value.content.clone() }
    }
}

fn extract_path(value: &SummaryMessageBlock) -> Option<String> {
    value.tool_call.as_ref().map(|tool_call| match tool_call {
        SummaryToolCall::FileRead { path } => path.to_owned(),
        SummaryToolCall::FileUpdate { path } => path.to_owned(),
        SummaryToolCall::FileRemove { path } => path.to_owned(),
    })
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

            for block in message.messages.drain(..) {
                // For tool calls, only keep successful operations
                if block.tool_call_success != Some(true) {
                    continue;
                }

                let path = extract_path(&block);

                let key = BlockKey::default().path(path);

                if let Some(value) = block_map.get(&key)
                    && value.content.is_none()
                {
                    block_map.shift_remove(&key);
                }
                block_map.insert(BlockKey::from(&block), block);
            }

            message.messages = block_map.into_values().collect();
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
    use crate::Role;
    use crate::compact::summary::{SummaryMessage, SummaryMessageBlock, SummaryToolCall};

    // Helper to create a summary message with role and blocks
    fn message(role: Role, blocks: Vec<SummaryMessageBlock>) -> SummaryMessage {
        SummaryMessage { role, messages: blocks }
    }

    // Helper to create a context summary
    fn summary(messages: Vec<SummaryMessage>) -> ContextSummary {
        ContextSummary { messages }
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
                Block::read("/test1"),
                Block::read("/test2"),
                Block::read("/test2"),
                Block::read("/test3"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test1"),
                Block::read("/test2"),
                Block::read("/test3"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_keeps_last_operation_with_content() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test").content("content1".to_string()),
                Block::read("/test").content("content2".to_string()),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test").content("content1".to_string()),
                Block::read("/test").content("content2".to_string()),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_different_operation_types_on_same_path() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test"),
                Block::read("/test"),
                Block::update("file.txt"),
                Block::update("file.txt"),
                Block::read("/test"),
                Block::update("/test"),
                Block::remove("/test"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![Block::update("file.txt"), Block::remove("/test")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_insertion_order() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/path1"),
                Block::read("/path2"),
                Block::read("/path1"),
                Block::update("/path3"),
                Block::read("/path2"),
                Block::remove("/path1"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::update("/path3"),
                Block::read("/path2"),
                Block::remove("/path1"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filters_failed_and_none_operations() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test").content("first read".to_string()),
                Block::read("/test")
                    .content("failed read".to_string())
                    .tool_call_success(false),
                Block::read("/test").content("second read".to_string()),
                Block::read("/unknown"),
                SummaryMessageBlock {
                    content: None,
                    tool_call_id: None,
                    tool_call: Some(SummaryToolCall::FileRead { path: "/unknown".to_string() }),
                    tool_call_success: None, // Should be ignored since success info is unavailable
                },
                Block::update("file.txt"),
                Block::read("/all_failed").tool_call_success(false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test").content("first read".to_string()),
                Block::read("/test").content("second read".to_string()),
                Block::read("/unknown"),
                Block::update("file.txt"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_assistant_message_after_filtering() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test1").tool_call_success(false),
                Block::read("/test2").tool_call_success(false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(Role::Assistant, vec![])]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_trims_assistant_messages() {
        let fixture = summary(vec![
            message(Role::User, vec![Block::read("/test"), Block::read("/test")]),
            message(
                Role::Assistant,
                vec![Block::update("file.txt"), Block::update("file.txt")],
            ),
            message(
                Role::System,
                vec![Block::remove("remove.txt"), Block::remove("remove.txt")],
            ),
            message(
                Role::Assistant,
                vec![Block::read("/test"), Block::read("/test")],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![Block::read("/test"), Block::read("/test")]),
            message(Role::Assistant, vec![Block::update("file.txt")]),
            message(
                Role::System,
                vec![Block::remove("remove.txt"), Block::remove("remove.txt")],
            ),
            message(Role::Assistant, vec![Block::read("/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_assistant_messages_trimmed_independently() {
        let fixture = summary(vec![
            message(
                Role::Assistant,
                vec![Block::read("/test"), Block::read("/test")],
            ),
            message(
                Role::Assistant,
                vec![Block::read("/test").tool_call_success(false)],
            ),
            message(
                Role::Assistant,
                vec![
                    Block::read("/test"),
                    Block::read("/test"),
                    Block::read("/test"),
                ],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![
            message(Role::Assistant, vec![Block::read("/test")]),
            message(Role::Assistant, vec![]),
            message(Role::Assistant, vec![Block::read("/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_assistant_message_with_content() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                Block::read("/test1"),
                Block::update("/test2"),
                Block::default().content("foo").tool_call_success(true),
                Block::read("/test1"),
                Block::read("/test2").content("bar"),
                Block::default().content("baz").tool_call_success(true),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                Block::default().content("foo").tool_call_success(true),
                Block::read("/test1"),
                Block::read("/test2").content("bar"),
                Block::default().content("baz").tool_call_success(true),
            ],
        )]);

        assert_eq!(actual, expected);
    }
}
