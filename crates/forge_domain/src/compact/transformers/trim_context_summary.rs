use crate::compact::summary::{ContextSummary, SummaryMessageContent, SummaryTool};
use crate::{Role, Transformer};

/// Removes redundant operations from the context summary.
///
/// This transformer deduplicates consecutive operations within assistant messages by
/// retaining only the most recent operation for each resource (e.g., file path, command).
/// Only applies to messages with the Assistant role. This is useful for reducing context
/// size while preserving the final state of operations.
pub struct TrimContextSummary;

/// Represents the type and target of a tool call operation.
///
/// Used for identifying and comparing operations to determine if they operate
/// on the same resource (e.g., same file path, same shell command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation<'a> {
    /// File operation (read, update, remove, undo) on a specific path
    File(&'a str),
    /// Shell command execution
    Shell(&'a str),
    /// Search operation with a specific pattern
    Search(&'a str),
    /// Fetch operation for a specific URL
    Fetch(&'a str),
    /// Follow-up question
    Followup(&'a str),
    /// Plan creation with a specific name
    Plan(&'a str),
}

impl SummaryTool {
    /// Converts the tool call to its operation type for comparison.
    ///
    /// File operations (read, update, remove, undo) on the same path are
    /// considered the same operation type for deduplication purposes.
    fn to_op(&self) -> Operation<'_> {
        match self {
            SummaryTool::FileRead { path } => Operation::File(path),
            SummaryTool::FileUpdate { path } => Operation::File(path),
            SummaryTool::FileRemove { path } => Operation::File(path),
            SummaryTool::Undo { path } => Operation::File(path),
            SummaryTool::Shell { command } => Operation::Shell(command),
            SummaryTool::Search { pattern } => Operation::Search(pattern),
            SummaryTool::Fetch { url } => Operation::Fetch(url),
            SummaryTool::Followup { question } => Operation::Followup(question),
            SummaryTool::Plan { plan_name } => Operation::Plan(plan_name),
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

            let mut block_seq: Vec<SummaryMessageContent> = Default::default();

            for block in message.contents.drain(..) {
                // For tool calls, only keep successful operations
                if let SummaryMessageContent::ToolCall(ref tool_call) = block {
                    // Remove previous entry if it has the same operation
                    if let Some(SummaryMessageContent::ToolCall(last_tool_call)) = block_seq.last_mut()
                        && last_tool_call.tool.to_op() == tool_call.tool.to_op()
                    {
                        block_seq.pop();
                    }
                }

                block_seq.push(block);
            }

            message.contents = block_seq;
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
    type Block = SummaryMessageContent;

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
                Block::read_with_status(None, "/unknown", false),
                Block::update(None, "file.txt"),
                Block::read_with_status(None, "/all_failed", false),
            ],
        )]);

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
            SummaryMessage::new(
                Role::Assistant,
                vec![Block::read_with_status(None, "/test", false)],
            ),
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

    #[test]
    fn test_preserves_shell_commands() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::shell(None, "cargo build"),
                Block::shell(None, "cargo test"),
                Block::shell(None, "cargo build"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::shell(None, "cargo build"),
                Block::shell(None, "cargo test"),
                Block::shell(None, "cargo build"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mixed_shell_and_file_operations() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo build"),
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo test"),
                Block::update(None, "/output.txt"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        // Shell commands break the deduplication chain, so both reads of /test.rs are
        // preserved
        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo build"),
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo test"),
                Block::update(None, "/output.txt"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_shell_commands_between_file_operations_on_same_path() {
        let fixture = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo build"),
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo test"),
                Block::read(None, "/test.rs"),
            ],
        )]);
        let actual = TrimContextSummary.transform(fixture);

        // Shell commands break the deduplication chain - all reads are preserved
        // because shell commands are interspersed between them
        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo build"),
                Block::read(None, "/test.rs"),
                Block::shell(None, "cargo test"),
                Block::read(None, "/test.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }
}
