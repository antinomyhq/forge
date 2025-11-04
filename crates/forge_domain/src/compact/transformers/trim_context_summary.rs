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
    fn test_merges_all_messages_in_summary() {
        let fixture = summary(vec![
            message(Role::Assistant, vec![read("/test"), read("/test")]),
            message(Role::User, vec![update("file.txt"), update("file.txt")]),
        ]);

        let actual = TrimContextSummary.transform(fixture);

        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(actual.messages[1].messages.len(), 2);
    }

    #[test]
    fn test_handles_empty_summary() {
        let fixture = summary(vec![]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages.len(), 0);
    }

    #[test]
    fn test_handles_messages_with_no_mergeable_blocks() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![read("/test1"), read("/test2"), read("/test3")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 3);
    }

    #[test]
    fn test_preserves_message_roles() {
        let fixture = summary(vec![
            message(Role::System, vec![]),
            message(Role::User, vec![]),
            message(Role::Assistant, vec![]),
        ]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].role, Role::System);
        assert_eq!(actual.messages[1].role, Role::User);
        assert_eq!(actual.messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_handles_mixed_mergeable_and_non_mergeable() {
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
        assert_eq!(actual.messages[0].messages.len(), 3);
    }

    #[test]
    fn test_merges_consecutive_identical_blocks() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![read("/test"), read("/test"), read("/test")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
    }

    #[test]
    fn test_does_not_merge_different_tool_calls() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![read("/test1"), read("/test2")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_filters_out_failed_operations() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![read("/test"), read("/test").tool_call_success(false)],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(actual.messages[0].messages[0].tool_call_success, Some(true));
    }

    #[test]
    fn test_keeps_last_operation_regardless_of_content() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test").content("content1"),
                read("/test").content("content2"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(
            actual.messages[0].messages[0].content,
            Some("content2".to_string())
        );
    }

    #[test]
    fn test_merges_different_tool_types_correctly() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test"),
                read("/test"),
                update("file.txt"),
                update("file.txt"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_does_not_trim_user_role_messages() {
        let fixture = summary(vec![message(
            Role::User,
            vec![read("/test"), read("/test"), read("/test")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 3);
    }

    #[test]
    fn test_does_not_trim_system_role_messages() {
        let fixture = summary(vec![message(
            Role::System,
            vec![update("file.txt"), update("file.txt")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 2);
    }

    #[test]
    fn test_only_trims_assistant_messages_in_mixed_roles() {
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
        ]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 2);
        assert_eq!(actual.messages[1].messages.len(), 1);
        assert_eq!(actual.messages[2].messages.len(), 2);
    }

    #[test]
    fn test_filters_out_all_failed_operations() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test1").tool_call_success(false),
                read("/test2").tool_call_success(false),
                update("file.txt").tool_call_success(false),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 0);
    }

    #[test]
    fn test_filters_out_operations_with_none_success_status() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                SummaryMessageBlock {
                    content: None,
                    tool_call_id: None,
                    tool_call: SummaryToolCall::FileRead { path: "/test".to_string() },
                    tool_call_success: None,
                },
                update("file.txt"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(
            actual.messages[0].messages[0].tool_call,
            SummaryToolCall::FileUpdate { path: "file.txt".to_string() }
        );
    }

    #[test]
    fn test_keeps_last_successful_operation_per_path() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/test").content("first read".to_string()),
                read("/test")
                    .content("failed read".to_string())
                    .tool_call_success(false),
                read("/test").content("second read".to_string()),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(
            actual.messages[0].messages[0].content,
            Some("second read".to_string())
        );
    }

    #[test]
    fn test_preserves_different_operation_types_on_same_path() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![read("/test"), update("/test"), remove("/test")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(
            actual.messages[0].messages[0].tool_call,
            SummaryToolCall::FileRemove { path: "/test".to_string() }
        );
    }

    #[test]
    fn test_multiple_paths_with_multiple_operations() {
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
        assert_eq!(actual.messages[0].messages.len(), 3);

        let path1_op = actual.messages[0]
            .messages
            .iter()
            .find(|b| TrimContextSummary::extract_path(&b.tool_call) == Some("/path1"))
            .unwrap();
        assert!(matches!(
            path1_op.tool_call,
            SummaryToolCall::FileRemove { .. }
        ));

        let path2_op = actual.messages[0]
            .messages
            .iter()
            .find(|b| TrimContextSummary::extract_path(&b.tool_call) == Some("/path2"))
            .unwrap();
        assert!(matches!(
            path2_op.tool_call,
            SummaryToolCall::FileRead { .. }
        ));

        let path3_op = actual.messages[0]
            .messages
            .iter()
            .find(|b| TrimContextSummary::extract_path(&b.tool_call) == Some("/path3"))
            .unwrap();
        assert!(matches!(
            path3_op.tool_call,
            SummaryToolCall::FileUpdate { .. }
        ));
    }

    #[test]
    fn test_preserves_insertion_order() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![read("/aaa"), read("/zzz"), read("/mmm")],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 3);
        assert_eq!(
            TrimContextSummary::extract_path(&actual.messages[0].messages[0].tool_call),
            Some("/aaa")
        );
        assert_eq!(
            TrimContextSummary::extract_path(&actual.messages[0].messages[1].tool_call),
            Some("/zzz")
        );
        assert_eq!(
            TrimContextSummary::extract_path(&actual.messages[0].messages[2].tool_call),
            Some("/mmm")
        );
    }

    #[test]
    fn test_mixed_success_and_failure_on_different_paths() {
        let fixture = summary(vec![message(
            Role::Assistant,
            vec![
                read("/success"),
                read("/failure").tool_call_success(false),
                SummaryMessageBlock {
                    content: None,
                    tool_call_id: None,
                    tool_call: SummaryToolCall::FileRead { path: "/unknown".to_string() },
                    tool_call_success: None,
                },
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(
            TrimContextSummary::extract_path(&actual.messages[0].messages[0].tool_call),
            Some("/success")
        );
    }

    #[test]
    fn test_multiple_assistant_messages_are_trimmed_independently() {
        let fixture = summary(vec![
            message(Role::Assistant, vec![read("/test"), read("/test")]),
            message(
                Role::Assistant,
                vec![read("/test"), read("/test"), read("/test")],
            ),
        ]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 1);
        assert_eq!(actual.messages[1].messages.len(), 1);
    }

    #[test]
    fn test_empty_assistant_message_after_filtering() {
        let fixture = summary(vec![
            message(
                Role::Assistant,
                vec![read("/test").tool_call_success(false)],
            ),
            message(Role::Assistant, vec![read("/other")]),
        ]);
        let actual = TrimContextSummary.transform(fixture);
        assert_eq!(actual.messages[0].messages.len(), 0);
        assert_eq!(actual.messages[1].messages.len(), 1);
    }
}
