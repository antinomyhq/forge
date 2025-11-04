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

    // Helper to create a FileRead block with default success=true
    fn read(path: &str) -> SummaryMessageBlock {
        SummaryMessageBlock {
            content: None,
            tool_call_id: None,
            tool_call: SummaryToolCall::FileRead { path: path.to_string() },
            tool_call_success: Some(true),
        }
    }

    // Helper to create a FileUpdate block with default success=true
    fn update(path: &str) -> SummaryMessageBlock {
        SummaryMessageBlock {
            content: None,
            tool_call_id: None,
            tool_call: SummaryToolCall::FileUpdate { path: path.to_string() },
            tool_call_success: Some(true),
        }
    }

    // Helper to create a FileRemove block with default success=true
    fn remove(path: &str) -> SummaryMessageBlock {
        SummaryMessageBlock {
            content: None,
            tool_call_id: None,
            tool_call: SummaryToolCall::FileRemove { path: path.to_string() },
            tool_call_success: Some(true),
        }
    }

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
                read("/test1"),
                read("/test2"),
                read("/test2"),
                read("/test3"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![read("/test1"), read("/test2"), read("/test3")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_keeps_last_operation_with_content() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test").content("content1".to_string()),
                read("/test").content("content2".to_string()),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![read("/test").content("content2".to_string())],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_different_operation_types_on_same_path() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test"),
                read("/test"),
                update("file.txt"),
                update("file.txt"),
                read("/test"),
                update("/test"),
                remove("/test"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![remove("/test"), update("file.txt")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_preserves_insertion_order() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/path1"),
                read("/path2"),
                read("/path1"),
                update("/path3"),
                read("/path2"),
                remove("/path1"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![remove("/path1"), read("/path2"), update("/path3")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filters_failed_and_none_operations() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test").content("first read".to_string()),
                read("/test")
                    .content("failed read".to_string())
                    .tool_call_success(false),
                read("/test").content("second read".to_string()),
                SummaryMessageBlock {
                    content: None,
                    tool_call_id: None,
                    tool_call: SummaryToolCall::FileRead { path: "/unknown".to_string() },
                    tool_call_success: None,
                },
                update("file.txt"),
                read("/all_failed").tool_call_success(false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test").content("second read".to_string()),
                update("file.txt"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_assistant_message_after_filtering() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test1").tool_call_success(false),
                read("/test2").tool_call_success(false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![message(Role::Assistant, vec![])]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_only_trims_assistant_messages() {
        let fixture = summary(vec![
            message(Role::User, vec![read("/test"), read("/test")]),
            message(
                Role::Assistant,
                vec![update("file.txt"), update("file.txt")],
            ),
            message(
                Role::System,
                vec![remove("remove.txt"), remove("remove.txt")],
            ),
            message(Role::Assistant, vec![read("/test"), read("/test")]),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![
            message(Role::User, vec![read("/test"), read("/test")]),
            message(Role::Assistant, vec![update("file.txt")]),
            message(
                Role::System,
                vec![remove("remove.txt"), remove("remove.txt")],
            ),
            message(Role::Assistant, vec![read("/test")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_assistant_messages_trimmed_independently() {
        let fixture = summary(vec![
            message(Role::Assistant, vec![read("/test"), read("/test")]),
            message(
                Role::Assistant,
                vec![read("/test").tool_call_success(false)],
            ),
            message(
                Role::Assistant,
                vec![read("/test"), read("/test"), read("/test")],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = summary(vec![
            message(Role::Assistant, vec![read("/test")]),
            message(Role::Assistant, vec![]),
            message(Role::Assistant, vec![read("/test")]),
        ]);

        assert_eq!(actual, expected);
    }
}
