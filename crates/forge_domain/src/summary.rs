use std::collections::HashMap;
use std::convert::identity;

use crate::{
    Context, ContextMessage, Role, TextMessage, ToolCallFull, ToolCallId, ToolResult, Tools,
};

/// A simplified summary of a context, focusing on messages and their tool calls
pub struct ContextSummary {
    pub messages: Vec<RoleMessage>,
}

/// A simplified representation of a message with its key information
pub struct RoleMessage {
    pub role: Role,
    pub message: Vec<SummaryMessage>,
}

trait CanMerge {
    fn can_merge(&self, other: &Self) -> bool;
}

impl CanMerge for RoleMessage {
    fn can_merge(&self, other: &Self) -> bool {
        self.role == other.role && self.message.can_merge(&other.message)
    }
}

impl RoleMessage {
    /// Merges consecutive messages that can be merged together.
    ///
    /// When the nth message can be merged with the (n-1)th message,
    /// the (n-1)th message is removed and replaced with the nth message.
    ///
    /// # Arguments
    ///
    /// * `messages` - A vector of SummaryMessage to merge
    ///
    /// # Returns
    ///
    /// A new vector with consecutive mergeable messages combined
    pub fn merge_consecutive(messages: Vec<Self>) -> Vec<Self> {
        let mut result: Vec<Self> = Vec::new();

        for message in messages {
            if let Some(last) = result.last_mut()
                && last.can_merge(&message)
            {
                // Replace the last message with the current message
                *last = message;
                continue;
            }
            result.push(message);
        }

        result
    }
}

impl CanMerge for Vec<SummaryMessage> {
    fn can_merge(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .zip(other)
                .all(|(this, that)| this.can_merge(that))
    }
}

impl CanMerge for SummaryMessage {
    fn can_merge(&self, other: &Self) -> bool {
        [
            self.content == other.content,
            self.tool_call_success == other.tool_call_success,
            self.tool_call.can_merge(&other.tool_call),
        ]
        .into_iter()
        .all(identity)
    }
}

impl CanMerge for SummaryToolCall {
    fn can_merge(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mcp { name: a }, Self::Mcp { name: b }) if a == b => true,
            (Self::FileRead { path: a }, Self::FileRead { path: b }) if a == b => true,
            (Self::FileUpdate { path: a }, Self::FileUpdate { path: b }) if a == b => true,
            (Self::FileRemove { path: a }, Self::FileRemove { path: b }) if a == b => true,
            (Self::Execute { cmd: a }, Self::Execute { cmd: b }) if a == b => true,
            (Self::Fetch { url: a }, Self::Fetch { url: b }) if a == b => true,
            _ => false,
        }
    }
}

/// Wraps tool call information along with its execution status
#[derive(Clone)]
pub struct SummaryMessage {
    pub content: Option<String>,
    pub tool_call_id: Option<ToolCallId>,
    pub tool_call: SummaryToolCall,
    pub tool_call_success: Option<bool>,
}

/// Categorized tool call information for summary purposes
#[derive(Clone)]
pub enum SummaryToolCall {
    Mcp { name: String },
    FileRead { path: String },
    FileUpdate { path: String },
    FileRemove { path: String },
    Execute { cmd: String },
    Fetch { url: String },
}

impl From<&Context> for ContextSummary {
    fn from(value: &Context) -> Self {
        let mut messages = vec![];
        let mut tool_results: HashMap<&ToolCallId, &ToolResult> = Default::default();

        for msg in &value.messages {
            match msg {
                ContextMessage::Text(text_msg) => {
                    let mut tool_info = Vec::<SummaryMessage>::from(text_msg);

                    // Set content on each SummaryMessage
                    for info in &mut tool_info {
                        info.content = Some(text_msg.content.clone());
                    }

                    messages.push(RoleMessage { role: text_msg.role.clone(), message: tool_info });
                }
                ContextMessage::Tool(tool_result) => {
                    if let Some(ref call_id) = tool_result.call_id {
                        tool_results.insert(call_id, tool_result);
                    }
                }
                ContextMessage::Image(_) => {
                    messages.push(RoleMessage { role: Role::User, message: vec![] });
                }
            }
        }

        messages
            .iter_mut()
            .flat_map(|message| message.message.iter_mut())
            .filter_map(|tool_info| {
                tool_info
                    .tool_call_id
                    .as_ref()
                    .and_then(|id| tool_results.get(id))
                    .map(|result| (result, tool_info))
            })
            .for_each(|(result, tool_info)| tool_info.tool_call_success = Some(!result.is_error()));

        ContextSummary { messages }
    }
}

impl From<&TextMessage> for Vec<SummaryMessage> {
    fn from(text_msg: &TextMessage) -> Self {
        text_msg
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|tool_call| {
                        extract_tool_info(tool_call).map(|call| SummaryMessage {
                            content: None,
                            tool_call_id: tool_call.call_id.clone(),
                            tool_call: call,
                            tool_call_success: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Extracts tool information from a tool call
fn extract_tool_info(call: &ToolCallFull) -> Option<SummaryToolCall> {
    // Handle MCP tools (tools starting with "mcp_")
    if call.name.as_str().starts_with("mcp_") {
        return Some(SummaryToolCall::Mcp { name: call.name.to_string() });
    }

    // Try to parse as a Tools enum variant
    let tool = Tools::try_from(call.clone()).ok()?;

    match tool {
        Tools::Read(input) => Some(SummaryToolCall::FileRead { path: input.path }),
        Tools::ReadImage(input) => Some(SummaryToolCall::FileRead { path: input.path }),
        Tools::Write(input) => Some(SummaryToolCall::FileUpdate { path: input.path }),
        Tools::Patch(input) => Some(SummaryToolCall::FileUpdate { path: input.path }),
        Tools::Remove(input) => Some(SummaryToolCall::FileRemove { path: input.path }),
        Tools::Shell(input) => Some(SummaryToolCall::Execute { cmd: input.command }),
        Tools::Fetch(input) => Some(SummaryToolCall::Fetch { url: input.url }),
        // Other tools don't have specific summary info
        Tools::Undo(_) | Tools::Followup(_) | Tools::Plan(_) | Tools::Search(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_summary_message(role: Role, tool_info: Vec<SummaryMessage>) -> RoleMessage {
        RoleMessage { role, message: tool_info }
    }

    fn fixture_tool_info(
        call: SummaryToolCall,
        success: Option<bool>,
        content: Option<String>,
    ) -> SummaryMessage {
        SummaryMessage {
            content,
            tool_call_id: None,
            tool_call: call,
            tool_call_success: success,
        }
    }

    #[test]
    fn test_merge_consecutive_empty_list() {
        let fixture: Vec<RoleMessage> = vec![];
        let actual = RoleMessage::merge_consecutive(fixture);
        let expected: Vec<RoleMessage> = vec![];
        assert_eq!(actual.len(), expected.len());
    }

    #[test]
    fn test_merge_consecutive_single_message() {
        let fixture = vec![fixture_summary_message(Role::Assistant, vec![])];
        let actual = RoleMessage::merge_consecutive(fixture);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].role, Role::Assistant);
    }

    #[test]
    fn test_merge_consecutive_mergeable_messages() {
        let tool1 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("Test".to_string()),
        );
        let tool2 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file2.txt".to_string() },
            Some(true),
            Some("Test".to_string()),
        );

        let fixture = vec![
            fixture_summary_message(Role::Assistant, vec![tool1]),
            fixture_summary_message(Role::Assistant, vec![tool2]),
        ];

        let actual = RoleMessage::merge_consecutive(fixture);
        let expected_len = 2;

        assert_eq!(actual.len(), expected_len);
        // Messages should NOT merge because they have different tool calls
        assert_eq!(actual[0].message.len(), 1);
        if let SummaryToolCall::FileRead { path } = &actual[0].message[0].tool_call {
            assert_eq!(path, "file1.txt");
        }
        assert_eq!(actual[1].message.len(), 1);
        if let SummaryToolCall::FileRead { path } = &actual[1].message[0].tool_call {
            assert_eq!(path, "file2.txt");
        }
    }

    #[test]
    fn test_merge_consecutive_non_mergeable_different_roles() {
        let fixture = vec![
            fixture_summary_message(Role::Assistant, vec![]),
            fixture_summary_message(Role::User, vec![]),
        ];

        let actual = RoleMessage::merge_consecutive(fixture);
        let expected_len = 2;

        assert_eq!(actual.len(), expected_len);
    }

    #[test]
    fn test_merge_consecutive_non_mergeable_different_content() {
        let tool1 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("Hello".to_string()),
        );
        let tool2 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("World".to_string()),
        );

        let fixture = vec![
            fixture_summary_message(Role::Assistant, vec![tool1]),
            fixture_summary_message(Role::Assistant, vec![tool2]),
        ];

        let actual = RoleMessage::merge_consecutive(fixture);
        let expected_len = 2;

        assert_eq!(actual.len(), expected_len);
    }

    #[test]
    fn test_merge_consecutive_multiple_groups() {
        let tool1 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("A".to_string()),
        );
        let tool2 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("A".to_string()),
        );
        let tool3 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file3.txt".to_string() },
            Some(true),
            Some("C".to_string()),
        );
        let tool4 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file3.txt".to_string() },
            Some(true),
            Some("C".to_string()),
        );

        let fixture = vec![
            // Group 1: mergeable
            fixture_summary_message(Role::Assistant, vec![tool1.clone()]),
            fixture_summary_message(Role::Assistant, vec![tool2]),
            // Different content - breaks merge
            fixture_summary_message(Role::Assistant, vec![]),
            // Group 2: mergeable
            fixture_summary_message(Role::User, vec![]),
            fixture_summary_message(Role::User, vec![tool3.clone()]),
            fixture_summary_message(Role::User, vec![tool4]),
        ];

        let actual = RoleMessage::merge_consecutive(fixture);
        let expected_len = 4;

        assert_eq!(actual.len(), expected_len);
        // Group 1: messages with same tool call merge
        assert_eq!(actual[0].message.len(), 1);
        if let SummaryToolCall::FileRead { path } = &actual[0].message[0].tool_call {
            assert_eq!(path, "file1.txt");
        }
        assert_eq!(actual[0].message[0].content, Some("A".to_string()));
        // Empty message group
        assert_eq!(actual[1].message.len(), 0);
        // Empty message group
        assert_eq!(actual[2].message.len(), 0);
        // Group 2: messages with same tool call merge
        assert_eq!(actual[3].message.len(), 1);
        if let SummaryToolCall::FileRead { path } = &actual[3].message[0].tool_call {
            assert_eq!(path, "file3.txt");
        }
        assert_eq!(actual[3].message[0].content, Some("C".to_string()));
    }

    #[test]
    fn test_merge_consecutive_replaces_with_last() {
        let tool1 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("Test".to_string()),
        );
        let tool2 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("Test".to_string()),
        );
        let tool3 = fixture_tool_info(
            SummaryToolCall::FileRead { path: "file1.txt".to_string() },
            Some(true),
            Some("Test".to_string()),
        );

        let fixture = vec![
            fixture_summary_message(Role::Assistant, vec![tool1]),
            fixture_summary_message(Role::Assistant, vec![tool2]),
            fixture_summary_message(Role::Assistant, vec![tool3]),
        ];

        let actual = RoleMessage::merge_consecutive(fixture);

        assert_eq!(actual.len(), 1);
        // The last message replaces all previous mergeable messages
        assert_eq!(actual[0].message.len(), 1);
        if let SummaryToolCall::FileRead { path } = &actual[0].message[0].tool_call {
            assert_eq!(path, "file1.txt");
        }
        assert_eq!(actual[0].message[0].content, Some("Test".to_string()));
    }
}
