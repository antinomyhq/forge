use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    Context, ContextMessage, Role, TextMessage, ToolCallFull, ToolCallId, ToolResult, Tools,
};

/// A simplified summary of a context, focusing on messages and their tool calls
#[derive(Default, PartialEq, Debug, Serialize, Deserialize, derive_setters::Setters)]
#[setters(strip_option)]
#[serde(rename_all = "snake_case")]
pub struct ContextSummary {
    pub messages: Vec<SummaryMessage>,
}

/// A simplified representation of a message with its key information
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, derive_setters::Setters)]
#[setters(strip_option)]
#[serde(rename_all = "snake_case")]
pub struct SummaryMessage {
    pub role: Role,
    pub blocks: Vec<SummaryMessageBlock>,
}

/// A message block that can be either content or a tool call
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryMessageBlock {
    Content(String),
    ToolCall(SummaryToolData),
}

/// Tool call data with execution status
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummaryToolData {
    pub tool_call_id: Option<ToolCallId>,
    pub call: SummaryToolCall,
    pub tool_call_success: bool,
}

impl ContextSummary {
    /// Creates a new ContextSummary with the given messages
    pub fn new(messages: Vec<SummaryMessage>) -> Self {
        Self { messages }
    }
}

impl SummaryMessage {
    /// Creates a new SummaryMessage with the given role and blocks
    pub fn new(role: Role, blocks: Vec<SummaryMessageBlock>) -> Self {
        Self { role, blocks }
    }
}

impl SummaryMessageBlock {
    /// Creates a content block
    pub fn content(text: impl Into<String>) -> Self {
        Self::Content(text.into())
    }

    /// Creates a FileRead tool call block with success=true by default
    pub fn read(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::FileRead { path: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates a FileRead tool call block with custom success status
    pub fn read_with_status(
        call_id: Option<ToolCallId>,
        path: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::FileRead { path: path.into() },
            tool_call_success: success,
        })
    }

    /// Creates a FileUpdate tool call block with success=true by default
    pub fn update(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::FileUpdate { path: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates a FileUpdate tool call block with custom success status
    pub fn update_with_status(
        call_id: Option<ToolCallId>,
        path: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::FileUpdate { path: path.into() },
            tool_call_success: success,
        })
    }

    /// Creates a FileRemove tool call block with success=true by default
    pub fn remove(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::FileRemove { path: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates a FileRemove tool call block with custom success status
    pub fn remove_with_status(
        call_id: Option<ToolCallId>,
        path: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::FileRemove { path: path.into() },
            tool_call_success: success,
        })
    }

    /// Creates a Shell tool call block with success=true by default
    pub fn shell(call_id: Option<ToolCallId>, command: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Shell { command: command.into() },
            tool_call_success: true,
        })
    }

    /// Creates a Shell tool call block with custom success status
    pub fn shell_with_status(
        call_id: Option<ToolCallId>,
        command: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Shell { command: command.into() },
            tool_call_success: success,
        })
    }

    /// Creates a Search tool call block with success=true by default
    pub fn search(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Search { pattern: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates a Search tool call block with custom success status
    pub fn search_with_status(
        call_id: Option<ToolCallId>,
        path: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Search { pattern: path.into() },
            tool_call_success: success,
        })
    }

    /// Creates an Undo tool call block with success=true by default
    pub fn undo(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Undo { path: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates an Undo tool call block with custom success status
    pub fn undo_with_status(
        call_id: Option<ToolCallId>,
        path: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Undo { path: path.into() },
            tool_call_success: success,
        })
    }

    /// Creates a Fetch tool call block with success=true by default
    pub fn fetch(call_id: Option<ToolCallId>, url: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Fetch { url: url.into() },
            tool_call_success: true,
        })
    }

    /// Creates a Fetch tool call block with custom success status
    pub fn fetch_with_status(
        call_id: Option<ToolCallId>,
        url: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Fetch { url: url.into() },
            tool_call_success: success,
        })
    }

    /// Creates a Followup tool call block with success=true by default
    pub fn followup(call_id: Option<ToolCallId>, question: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Followup { question: question.into() },
            tool_call_success: true,
        })
    }

    /// Creates a Followup tool call block with custom success status
    pub fn followup_with_status(
        call_id: Option<ToolCallId>,
        question: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Followup { question: question.into() },
            tool_call_success: success,
        })
    }

    /// Creates a Plan tool call block with success=true by default
    pub fn plan(call_id: Option<ToolCallId>, plan_name: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Plan { plan_name: plan_name.into() },
            tool_call_success: true,
        })
    }

    /// Creates a Plan tool call block with custom success status
    pub fn plan_with_status(
        call_id: Option<ToolCallId>,
        plan_name: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            call: SummaryToolCall::Plan { plan_name: plan_name.into() },
            tool_call_success: success,
        })
    }
}

/// Categorized tool call information for summary purposes
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryToolCall {
    FileRead { path: String },
    FileUpdate { path: String },
    FileRemove { path: String },
    Shell { command: String },
    Search { pattern: String },
    Undo { path: String },
    Fetch { url: String },
    Followup { question: String },
    Plan { plan_name: String },
}

impl From<&Context> for ContextSummary {
    fn from(value: &Context) -> Self {
        let mut messages = vec![];
        let mut buffer: Vec<SummaryMessageBlock> = vec![];
        let mut tool_results: HashMap<&ToolCallId, &ToolResult> = Default::default();
        let mut current_role = Role::System;
        for msg in &value.messages {
            match msg {
                ContextMessage::Text(text_msg) => {
                    // Skip system messages
                    if text_msg.role == Role::System {
                        continue;
                    }

                    if current_role != text_msg.role {
                        // Only push if buffer is not empty (avoid empty System role at start)
                        if !buffer.is_empty() {
                            messages.push(SummaryMessage {
                                role: current_role,
                                blocks: std::mem::take(&mut buffer),
                            });
                        }

                        current_role = text_msg.role;
                    }

                    buffer.extend(Vec::<SummaryMessageBlock>::from(text_msg));
                }
                ContextMessage::Tool(tool_result) => {
                    if let Some(ref call_id) = tool_result.call_id {
                        tool_results.insert(call_id, tool_result);
                    }
                }
                ContextMessage::Image(_) => {}
            }
        }

        // Insert the last chunk if buffer is not empty
        if !buffer.is_empty() {
            messages
                .push(SummaryMessage { role: current_role, blocks: std::mem::take(&mut buffer) });
        }

        // Update tool call success status based on results
        messages
            .iter_mut()
            .flat_map(|message| message.blocks.iter_mut())
            .for_each(|block| {
                if let SummaryMessageBlock::ToolCall(tool_data) = block
                    && let Some(call_id) = &tool_data.tool_call_id
                    && let Some(result) = tool_results.get(call_id)
                {
                    tool_data.tool_call_success = !result.is_error();
                }
            });

        ContextSummary { messages }
    }
}

impl From<&TextMessage> for Vec<SummaryMessageBlock> {
    fn from(text_msg: &TextMessage) -> Self {
        let mut blocks = vec![];

        // Add content block if there's text content
        if !text_msg.content.is_empty() {
            blocks.push(SummaryMessageBlock::Content(text_msg.content.clone()));
        }

        // Add tool call blocks if present
        if let Some(calls) = &text_msg.tool_calls {
            blocks.extend(calls.iter().filter_map(|tool_call| {
                extract_tool_info(tool_call).map(|call| {
                    SummaryMessageBlock::ToolCall(SummaryToolData {
                        tool_call_id: tool_call.call_id.clone(),
                        call,
                        tool_call_success: false,
                    })
                })
            }));
        }

        blocks
    }
}

/// Extracts tool information from a tool call
fn extract_tool_info(call: &ToolCallFull) -> Option<SummaryToolCall> {
    // Try to parse as a Tools enum variant
    let tool = Tools::try_from(call.clone()).ok()?;

    match tool {
        Tools::Read(input) => Some(SummaryToolCall::FileRead { path: input.path }),
        Tools::ReadImage(input) => Some(SummaryToolCall::FileRead { path: input.path }),
        Tools::Write(input) => Some(SummaryToolCall::FileUpdate { path: input.path }),
        Tools::Patch(input) => Some(SummaryToolCall::FileUpdate { path: input.path }),
        Tools::Remove(input) => Some(SummaryToolCall::FileRemove { path: input.path }),
        Tools::Shell(input) => Some(SummaryToolCall::Shell { command: input.command }),
        Tools::Search(input) => input
            .file_pattern
            .or(input.regex)
            .map(|pattern| SummaryToolCall::Search { pattern }),
        Tools::Undo(input) => Some(SummaryToolCall::Undo { path: input.path }),
        Tools::Fetch(input) => Some(SummaryToolCall::Fetch { url: input.url }),
        Tools::Followup(input) => Some(SummaryToolCall::Followup { question: input.question }),
        Tools::Plan(input) => Some(SummaryToolCall::Plan { plan_name: input.plan_name }),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{ContextMessage, TextMessage, ToolCallArguments, ToolCallId, ToolName, ToolOutput};

    type Block = SummaryMessageBlock;

    fn context(messages: Vec<ContextMessage>) -> Context {
        Context::default().messages(messages)
    }

    fn user(content: impl Into<String>) -> ContextMessage {
        ContextMessage::Text(TextMessage {
            role: Role::User,
            content: content.into(),
            raw_content: None,
            tool_calls: None,
            model: None,
            reasoning_details: None,
        })
    }

    fn assistant(content: impl Into<String>) -> ContextMessage {
        ContextMessage::Text(TextMessage {
            role: Role::Assistant,
            content: content.into(),
            raw_content: None,
            tool_calls: None,
            model: None,
            reasoning_details: None,
        })
    }

    fn assistant_with_tools(
        content: impl Into<String>,
        tool_calls: Vec<ToolCallFull>,
    ) -> ContextMessage {
        ContextMessage::Text(TextMessage {
            role: Role::Assistant,
            content: content.into(),
            raw_content: None,
            tool_calls: Some(tool_calls),
            model: None,
            reasoning_details: None,
        })
    }

    fn system(content: impl Into<String>) -> ContextMessage {
        ContextMessage::Text(TextMessage {
            role: Role::System,
            content: content.into(),
            raw_content: None,
            tool_calls: None,
            model: None,
            reasoning_details: None,
        })
    }

    fn tool_call(name: &str, call_id: &str, path: &str) -> ToolCallFull {
        let args = format!(r#"{{"path": "{}"}}"#, path);
        ToolCallFull {
            name: ToolName::new(name),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_write(call_id: &str, path: &str, content: &str) -> ToolCallFull {
        let args = format!(r#"{{"path": "{}", "content": "{}"}}"#, path, content);
        ToolCallFull {
            name: ToolName::new("write"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_patch(call_id: &str, path: &str) -> ToolCallFull {
        let args = format!(
            r#"{{"path": "{}", "search": "old", "content": "new", "operation": "replace"}}"#,
            path
        );
        ToolCallFull {
            name: ToolName::new("patch"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_shell(call_id: &str, command: &str) -> ToolCallFull {
        let args = format!(r#"{{"command": "{}", "cwd": "/test"}}"#, command);
        ToolCallFull {
            name: ToolName::new("shell"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_search(call_id: &str, pattern: &str) -> ToolCallFull {
        let args = format!(r#"{{"path": "/test", "regex": "{}"}}"#, pattern);
        ToolCallFull {
            name: ToolName::new("search"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_undo(call_id: &str, path: &str) -> ToolCallFull {
        let args = format!(r#"{{"path": "{}"}}"#, path);
        ToolCallFull {
            name: ToolName::new("undo"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_fetch(call_id: &str, url: &str) -> ToolCallFull {
        let args = format!(r#"{{"url": "{}"}}"#, url);
        ToolCallFull {
            name: ToolName::new("fetch"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_followup(call_id: &str, question: &str) -> ToolCallFull {
        let args = format!(r#"{{"question": "{}"}}"#, question);
        ToolCallFull {
            name: ToolName::new("followup"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_call_plan(call_id: &str, plan_name: &str) -> ToolCallFull {
        let args = format!(
            r#"{{"plan_name": "{}", "version": "v1", "content": "test"}}"#,
            plan_name
        );
        ToolCallFull {
            name: ToolName::new("plan"),
            call_id: Some(ToolCallId::new(call_id)),
            arguments: ToolCallArguments::from_json(&args),
        }
    }

    fn tool_result(name: &str, call_id: &str, is_error: bool) -> ContextMessage {
        ContextMessage::Tool(ToolResult {
            name: ToolName::new(name),
            call_id: Some(ToolCallId::new(call_id)),
            output: ToolOutput::text("result").is_error(is_error),
        })
    }

    #[test]
    fn test_summary_message_block_read_helper() {
        let actual = Block::read(None, "/path/to/file.rs");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::FileRead { path: "/path/to/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_update_helper() {
        let actual = Block::update(None, "/path/to/file.rs");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::FileUpdate { path: "/path/to/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_remove_helper() {
        let actual = Block::remove(None, "/path/to/file.rs");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::FileRemove { path: "/path/to/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_empty_context() {
        let fixture = Context::default();
        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::default();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_user_and_assistant_without_tools() {
        let fixture = context(vec![
            user("Please help me"),
            assistant("Sure, I can help"),
            user("Thanks"),
            assistant("You're welcome"),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![Block::content("Please help me")]),
            SummaryMessage::new(Role::Assistant, vec![Block::content("Sure, I can help")]),
            SummaryMessage::new(Role::User, vec![Block::content("Thanks")]),
            SummaryMessage::new(Role::Assistant, vec![Block::content("You're welcome")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_skips_system_messages() {
        let fixture = context(vec![system("System prompt"), user("User message")]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::User,
            vec![Block::content("User message")],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_read_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Reading file",
            vec![tool_call("read", "call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Reading file"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_write_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Writing file",
            vec![tool_call_write("call_1", "/test/file.rs", "test")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Writing file"),
                Block::update_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_patch_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Patching file",
            vec![tool_call_patch("call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Patching file"),
                Block::update_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_remove_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Removing file",
            vec![tool_call("remove", "call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Removing file"),
                Block::remove_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_read_image_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Reading image",
            vec![tool_call("read_image", "call_1", "/test/image.png")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Reading image"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/image.png", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_shell_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Running shell",
            vec![tool_call_shell("call_1", "ls -la")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Running shell"),
                Block::shell_with_status(Some(ToolCallId::new("call_1")), "ls -la", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_multiple_tool_calls_in_message() {
        let fixture = context(vec![assistant_with_tools(
            "Multiple operations",
            vec![
                tool_call("read", "call_1", "/test/file1.rs"),
                tool_call_write("call_2", "/test/file2.rs", "test"),
                tool_call("remove", "call_3", "/test/file3.rs"),
            ],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Multiple operations"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file1.rs", false),
                Block::update_with_status(Some(ToolCallId::new("call_2")), "/test/file2.rs", false),
                Block::remove_with_status(Some(ToolCallId::new("call_3")), "/test/file3.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_tool_results_to_calls_success() {
        let fixture = context(vec![
            assistant_with_tools(
                "Reading file",
                vec![tool_call("read", "call_1", "/test/file.rs")],
            ),
            tool_result("read", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Reading file"),
                Block::read(Some(ToolCallId::new("call_1")), "/test/file.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_tool_results_to_calls_failure() {
        let fixture = context(vec![
            assistant_with_tools(
                "Reading file",
                vec![tool_call("read", "call_1", "/test/file.rs")],
            ),
            tool_result("read", "call_1", true),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Reading file"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_multiple_tool_results() {
        let fixture = context(vec![
            assistant_with_tools(
                "Multiple operations",
                vec![
                    tool_call("read", "call_1", "/test/file1.rs"),
                    tool_call_write("call_2", "/test/file2.rs", "test"),
                ],
            ),
            tool_result("read", "call_1", false),
            tool_result("write", "call_2", true),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Multiple operations"),
                Block::read(Some(ToolCallId::new("call_1")), "/test/file1.rs"),
                Block::update_with_status(Some(ToolCallId::new("call_2")), "/test/file2.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_tool_result_without_call_id() {
        let fixture = context(vec![
            assistant_with_tools(
                "Reading file",
                vec![tool_call("read", "call_1", "/test/file.rs")],
            ),
            ContextMessage::Tool(ToolResult {
                name: ToolName::new("read"),
                call_id: None,
                output: ToolOutput::text("result"),
            }),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Reading file"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_complex_conversation() {
        let fixture = context(vec![
            system("System prompt"),
            user("Read this file"),
            assistant_with_tools(
                "Reading",
                vec![tool_call("read", "call_1", "/test/file1.rs")],
            ),
            tool_result("read", "call_1", false),
            user("Now update it"),
            assistant_with_tools(
                "Updating",
                vec![tool_call_write("call_2", "/test/file1.rs", "new content")],
            ),
            tool_result("write", "call_2", false),
            assistant("Done"),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![Block::content("Read this file")]),
            SummaryMessage::new(
                Role::Assistant,
                vec![
                    Block::content("Reading"),
                    Block::read(Some(ToolCallId::new("call_1")), "/test/file1.rs"),
                ],
            ),
            SummaryMessage::new(Role::User, vec![Block::content("Now update it")]),
            SummaryMessage::new(
                Role::Assistant,
                vec![
                    Block::content("Updating"),
                    Block::update(Some(ToolCallId::new("call_2")), "/test/file1.rs"),
                    Block::content("Done"),
                ],
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_ignores_image_messages() {
        let fixture = context(vec![
            user("User message"),
            ContextMessage::Image(crate::Image::new_base64(
                "test_image_data".to_string(),
                "image/png",
            )),
            assistant("Assistant"),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![
            SummaryMessage::new(Role::User, vec![Block::content("User message")]),
            SummaryMessage::new(Role::Assistant, vec![Block::content("Assistant")]),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_tool_info_with_invalid_tool() {
        let fixture = ToolCallFull {
            name: ToolName::new("invalid_tool"),
            call_id: Some(ToolCallId::new("call_1")),
            arguments: ToolCallArguments::from_json(r#"{"invalid": "args"}"#),
        };

        let actual = extract_tool_info(&fixture);

        assert_eq!(actual, None);
    }

    #[test]
    fn test_summary_message_block_shell_helper() {
        let actual = Block::shell(None, "cargo build");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::Shell { command: "cargo build".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_shell_results_to_calls() {
        let fixture = context(vec![
            assistant_with_tools(
                "Running command",
                vec![tool_call_shell("call_1", "echo test")],
            ),
            tool_result("shell", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Running command"),
                Block::shell(Some(ToolCallId::new("call_1")), "echo test"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_mixed_file_and_shell_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Multiple operations",
            vec![
                tool_call("read", "call_1", "/test/file.rs"),
                tool_call_shell("call_2", "cargo test"),
                tool_call_write("call_3", "/test/output.txt", "result"),
            ],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Multiple operations"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
                Block::shell_with_status(Some(ToolCallId::new("call_2")), "cargo test", false),
                Block::update_with_status(
                    Some(ToolCallId::new("call_3")),
                    "/test/output.txt",
                    false,
                ),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_ignores_non_file_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Searching",
            vec![ToolCallFull {
                name: ToolName::new("search"),
                call_id: Some(ToolCallId::new("call_1")),
                arguments: ToolCallArguments::from_json(r#"{"path": "/test", "regex": "pattern"}"#),
            }],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Searching"),
                Block::search_with_status(Some(ToolCallId::new("call_1")), "pattern", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_search_helper() {
        let actual = Block::search(None, "/project/src");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::Search { pattern: "/project/src".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_search_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Searching files",
            vec![tool_call_search("call_1", "/test/src")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Searching files"),
                Block::search_with_status(Some(ToolCallId::new("call_1")), "/test/src", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_search_results_to_calls() {
        let fixture = context(vec![
            assistant_with_tools("Searching", vec![tool_call_search("call_1", "/test/src")]),
            tool_result("search", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Searching"),
                Block::search(Some(ToolCallId::new("call_1")), "/test/src"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_mixed_file_shell_and_search_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Multiple operations",
            vec![
                tool_call("read", "call_1", "/test/file.rs"),
                tool_call_shell("call_2", "cargo test"),
                tool_call_search("call_3", "/test/src"),
                tool_call_write("call_4", "/test/output.txt", "result"),
            ],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Multiple operations"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
                Block::shell_with_status(Some(ToolCallId::new("call_2")), "cargo test", false),
                Block::search_with_status(Some(ToolCallId::new("call_3")), "/test/src", false),
                Block::update_with_status(
                    Some(ToolCallId::new("call_4")),
                    "/test/output.txt",
                    false,
                ),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_undo_helper() {
        let actual = Block::undo(None, "/test/file.rs");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::Undo { path: "/test/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_fetch_helper() {
        let actual = Block::fetch(None, "https://example.com");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::Fetch { url: "https://example.com".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_followup_helper() {
        let actual = Block::followup(None, "What should I do next?");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::Followup { question: "What should I do next?".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_plan_helper() {
        let actual = Block::plan(None, "feature-implementation");

        let expected = Block::ToolCall(SummaryToolData {
            tool_call_id: None,
            call: SummaryToolCall::Plan { plan_name: "feature-implementation".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_undo_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Undoing changes",
            vec![tool_call_undo("call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Undoing changes"),
                Block::undo_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_fetch_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Fetching data",
            vec![tool_call_fetch("call_1", "https://api.example.com")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Fetching data"),
                Block::fetch_with_status(
                    Some(ToolCallId::new("call_1")),
                    "https://api.example.com",
                    false,
                ),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_followup_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Asking question",
            vec![tool_call_followup("call_1", "Should I proceed?")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Asking question"),
                Block::followup_with_status(
                    Some(ToolCallId::new("call_1")),
                    "Should I proceed?",
                    false,
                ),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_plan_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Creating plan",
            vec![tool_call_plan("call_1", "feature-plan")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Creating plan"),
                Block::plan_with_status(Some(ToolCallId::new("call_1")), "feature-plan", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_undo_results_to_calls() {
        let fixture = context(vec![
            assistant_with_tools("Undoing", vec![tool_call_undo("call_1", "/test/file.rs")]),
            tool_result("undo", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Undoing"),
                Block::undo(Some(ToolCallId::new("call_1")), "/test/file.rs"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_fetch_results_to_calls() {
        let fixture = context(vec![
            assistant_with_tools(
                "Fetching",
                vec![tool_call_fetch("call_1", "https://example.com")],
            ),
            tool_result("fetch", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Fetching"),
                Block::fetch(Some(ToolCallId::new("call_1")), "https://example.com"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_followup_results_to_calls() {
        let fixture = context(vec![
            assistant_with_tools("Asking", vec![tool_call_followup("call_1", "Continue?")]),
            tool_result("followup", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Asking"),
                Block::followup(Some(ToolCallId::new("call_1")), "Continue?"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_links_plan_results_to_calls() {
        let fixture = context(vec![
            assistant_with_tools("Planning", vec![tool_call_plan("call_1", "my-plan")]),
            tool_result("plan", "call_1", false),
        ]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("Planning"),
                Block::plan(Some(ToolCallId::new("call_1")), "my-plan"),
            ],
        )]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_all_tools_mixed() {
        let fixture = context(vec![assistant_with_tools(
            "All operations",
            vec![
                tool_call("read", "call_1", "/test/file.rs"),
                tool_call_write("call_2", "/test/output.txt", "content"),
                tool_call("remove", "call_3", "/test/old.txt"),
                tool_call_shell("call_4", "cargo build"),
                tool_call_search("call_5", "/test/src"),
                tool_call_undo("call_6", "/test/undo.txt"),
                tool_call_fetch("call_7", "https://example.com"),
                tool_call_followup("call_8", "Proceed?"),
                tool_call_plan("call_9", "implementation"),
            ],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary::new(vec![SummaryMessage::new(
            Role::Assistant,
            vec![
                Block::content("All operations"),
                Block::read_with_status(Some(ToolCallId::new("call_1")), "/test/file.rs", false),
                Block::update_with_status(
                    Some(ToolCallId::new("call_2")),
                    "/test/output.txt",
                    false,
                ),
                Block::remove_with_status(Some(ToolCallId::new("call_3")), "/test/old.txt", false),
                Block::shell_with_status(Some(ToolCallId::new("call_4")), "cargo build", false),
                Block::search_with_status(Some(ToolCallId::new("call_5")), "/test/src", false),
                Block::undo_with_status(Some(ToolCallId::new("call_6")), "/test/undo.txt", false),
                Block::fetch_with_status(
                    Some(ToolCallId::new("call_7")),
                    "https://example.com",
                    false,
                ),
                Block::followup_with_status(Some(ToolCallId::new("call_8")), "Proceed?", false),
                Block::plan_with_status(Some(ToolCallId::new("call_9")), "implementation", false),
            ],
        )]);

        assert_eq!(actual, expected);
    }
}
