use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    Context, ContextMessage, Role, TextMessage, ToolCallFull, ToolCallId, ToolResult, Tools,
};

/// A simplified summary of a context, focusing on messages and their tool calls
#[derive(PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextSummary {
    pub messages: Vec<SummaryMessage>,
}

/// A simplified representation of a message with its key information
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummaryMessage {
    pub role: Role,
    pub blocks: Vec<SummaryMessageBlock>,
}

/// A message block that can be either content or a tool call
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", untagged)]
pub enum SummaryMessageBlock {
    Content(String),
    ToolCall(SummaryToolData),
}

/// Tool call data with execution status
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummaryToolData {
    pub tool_call_id: Option<ToolCallId>,
    pub tool_call: SummaryToolCall,
    pub tool_call_success: bool,
}

impl SummaryMessageBlock {
    /// Creates a content block
    pub fn content(text: impl Into<String>) -> Self {
        Self::Content(text.into())
    }

    /// Creates a FileRead tool call block with unknown success status (defaults
    /// to false)
    pub fn read(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            tool_call: SummaryToolCall::FileRead { path: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates a FileUpdate tool call block with success=true by default
    pub fn update(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            tool_call: SummaryToolCall::FileUpdate { path: path.into() },
            tool_call_success: true,
        })
    }

    /// Creates a FileRemove tool call block with success=true by default
    pub fn remove(call_id: Option<ToolCallId>, path: impl Into<String>) -> Self {
        Self::ToolCall(SummaryToolData {
            tool_call_id: call_id,
            tool_call: SummaryToolCall::FileRemove { path: path.into() },
            tool_call_success: true,
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
                        tool_call: call,
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
        // Other tools don't have specific summary info
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{ContextMessage, TextMessage, ToolCallArguments, ToolCallId, ToolName, ToolOutput};

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

    fn tool_result(name: &str, call_id: &str, is_error: bool) -> ContextMessage {
        ContextMessage::Tool(ToolResult {
            name: ToolName::new(name),
            call_id: Some(ToolCallId::new(call_id)),
            output: ToolOutput::text("result").is_error(is_error),
        })
    }

    fn summary_msg(role: Role, blocks: Vec<SummaryMessageBlock>) -> SummaryMessage {
        SummaryMessage { role, blocks }
    }

    fn block_read(call_id: &str, path: &str, success: bool) -> SummaryMessageBlock {
        SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: Some(ToolCallId::new(call_id)),
            tool_call: SummaryToolCall::FileRead { path: path.to_string() },
            tool_call_success: success,
        })
    }

    fn block_read_with_content(
        content: &str,
        call_id: &str,
        path: &str,
        success: bool,
    ) -> Vec<SummaryMessageBlock> {
        vec![
            SummaryMessageBlock::Content(content.to_string()),
            SummaryMessageBlock::ToolCall(SummaryToolData {
                tool_call_id: Some(ToolCallId::new(call_id)),
                tool_call: SummaryToolCall::FileRead { path: path.to_string() },
                tool_call_success: success,
            }),
        ]
    }

    fn block_update(call_id: &str, path: &str, success: bool) -> SummaryMessageBlock {
        SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: Some(ToolCallId::new(call_id)),
            tool_call: SummaryToolCall::FileUpdate { path: path.to_string() },
            tool_call_success: success,
        })
    }

    fn block_update_with_content(
        content: &str,
        call_id: &str,
        path: &str,
        success: bool,
    ) -> Vec<SummaryMessageBlock> {
        vec![
            SummaryMessageBlock::Content(content.to_string()),
            SummaryMessageBlock::ToolCall(SummaryToolData {
                tool_call_id: Some(ToolCallId::new(call_id)),
                tool_call: SummaryToolCall::FileUpdate { path: path.to_string() },
                tool_call_success: success,
            }),
        ]
    }

    fn block_remove(call_id: &str, path: &str, success: bool) -> SummaryMessageBlock {
        SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: Some(ToolCallId::new(call_id)),
            tool_call: SummaryToolCall::FileRemove { path: path.to_string() },
            tool_call_success: success,
        })
    }

    fn block_content(content: impl Into<String>) -> SummaryMessageBlock {
        SummaryMessageBlock::Content(content.into())
    }

    #[test]
    fn test_summary_message_block_read_helper() {
        let actual = SummaryMessageBlock::read(None, "/path/to/file.rs");

        let expected = SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: None,
            tool_call: SummaryToolCall::FileRead { path: "/path/to/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_update_helper() {
        let actual = SummaryMessageBlock::update(None, "/path/to/file.rs");

        let expected = SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: None,
            tool_call: SummaryToolCall::FileUpdate { path: "/path/to/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_summary_message_block_remove_helper() {
        let actual = SummaryMessageBlock::remove(None, "/path/to/file.rs");

        let expected = SummaryMessageBlock::ToolCall(SummaryToolData {
            tool_call_id: None,
            tool_call: SummaryToolCall::FileRemove { path: "/path/to/file.rs".to_string() },
            tool_call_success: true,
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_empty_context() {
        let fixture = Context::default();

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary { messages: vec![] };

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

        let expected = ContextSummary {
            messages: vec![
                summary_msg(Role::User, vec![block_content("Please help me")]),
                summary_msg(Role::Assistant, vec![block_content("Sure, I can help")]),
                summary_msg(Role::User, vec![block_content("Thanks")]),
                summary_msg(Role::Assistant, vec![block_content("You're welcome")]),
            ],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_skips_system_messages() {
        let fixture = context(vec![system("System prompt"), user("User message")]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(Role::User, vec![block_content("User message")])],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_read_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Reading file",
            vec![tool_call("read", "call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Reading file"),
                    block_read("call_1", "/test/file.rs", false),
                ],
            )],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_write_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Writing file",
            vec![tool_call_write("call_1", "/test/file.rs", "test")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Writing file"),
                    block_update("call_1", "/test/file.rs", false),
                ],
            )],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_patch_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Patching file",
            vec![tool_call_patch("call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Patching file"),
                    block_update("call_1", "/test/file.rs", false),
                ],
            )],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_file_remove_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Removing file",
            vec![tool_call("remove", "call_1", "/test/file.rs")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Removing file"),
                    block_remove("call_1", "/test/file.rs", false),
                ],
            )],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_extracts_read_image_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Reading image",
            vec![tool_call("read_image", "call_1", "/test/image.png")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Reading image"),
                    block_read("call_1", "/test/image.png", false),
                ],
            )],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_context_summary_ignores_non_file_tool_calls() {
        let fixture = context(vec![assistant_with_tools(
            "Running shell",
            vec![tool_call_shell("call_1", "ls")],
        )]);

        let actual = ContextSummary::from(&fixture);

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![block_content("Running shell")],
            )],
        };

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

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Multiple operations"),
                    block_read("call_1", "/test/file1.rs", false),
                    block_update("call_2", "/test/file2.rs", false),
                    block_remove("call_3", "/test/file3.rs", false),
                ],
            )],
        };

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

        let expected_blocks = vec![
            block_content("Reading file"),
            block_read("call_1", "/test/file.rs", true),
        ];

        let expected = ContextSummary {
            messages: vec![summary_msg(Role::Assistant, expected_blocks)],
        };

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

        let expected_blocks = vec![
            block_content("Reading file"),
            block_read("call_1", "/test/file.rs", false),
        ];

        let expected = ContextSummary {
            messages: vec![summary_msg(Role::Assistant, expected_blocks)],
        };

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

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Multiple operations"),
                    block_read("call_1", "/test/file1.rs", true),
                    block_update("call_2", "/test/file2.rs", false),
                ],
            )],
        };

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

        let expected = ContextSummary {
            messages: vec![summary_msg(
                Role::Assistant,
                vec![
                    block_content("Reading file"),
                    block_read("call_1", "/test/file.rs", false),
                ],
            )],
        };

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

        let expected = ContextSummary {
            messages: vec![
                summary_msg(Role::User, vec![block_content("Read this file")]),
                summary_msg(
                    Role::Assistant,
                    vec![
                        block_content("Reading"),
                        block_read("call_1", "/test/file1.rs", true),
                    ],
                ),
                summary_msg(Role::User, vec![block_content("Now update it")]),
                summary_msg(
                    Role::Assistant,
                    vec![
                        block_content("Updating"),
                        block_update("call_2", "/test/file1.rs", true),
                        block_content("Done"),
                    ],
                ),
            ],
        };

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

        let expected = ContextSummary {
            messages: vec![
                summary_msg(Role::User, vec![block_content("User message")]),
                summary_msg(Role::Assistant, vec![block_content("Assistant")]),
            ],
        };

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
}
