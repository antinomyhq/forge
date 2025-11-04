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
    pub messages: Vec<SummaryMessageBlock>,
}

/// Wraps tool call information along with its execution status
#[derive(Default, Clone, PartialEq, Debug, derive_setters::Setters, Serialize, Deserialize)]
#[setters(strip_option, into)]
#[serde(rename_all = "snake_case")]
pub struct SummaryMessageBlock {
    pub content: Option<String>,
    pub tool_call_id: Option<ToolCallId>,
    pub tool_call: Option<SummaryToolCall>,
    pub tool_call_success: Option<bool>,
}

impl SummaryMessageBlock {
    /// Creates a FileRead tool call block with success=true by default
    pub fn read(path: impl Into<String>) -> Self {
        Self {
            content: None,
            tool_call_id: None,
            tool_call: Some(SummaryToolCall::FileRead { path: path.into() }),
            tool_call_success: Some(true),
        }
    }

    /// Creates a FileUpdate tool call block with success=true by default
    pub fn update(path: impl Into<String>) -> Self {
        Self {
            content: None,
            tool_call_id: None,
            tool_call: Some(SummaryToolCall::FileUpdate { path: path.into() }),
            tool_call_success: Some(true),
        }
    }

    /// Creates a FileRemove tool call block with success=true by default
    pub fn remove(path: impl Into<String>) -> Self {
        Self {
            content: None,
            tool_call_id: None,
            tool_call: Some(SummaryToolCall::FileRemove { path: path.into() }),
            tool_call_success: Some(true),
        }
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
                                messages: std::mem::take(&mut buffer),
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
                .push(SummaryMessage { role: current_role, messages: std::mem::take(&mut buffer) });
        }

        messages
            .iter_mut()
            .flat_map(|message| message.messages.iter_mut())
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

impl From<&TextMessage> for Vec<SummaryMessageBlock> {
    fn from(text_msg: &TextMessage) -> Self {
        text_msg
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|tool_call| {
                        extract_tool_info(tool_call).map(|call| SummaryMessageBlock {
                            content: None,
                            tool_call_id: tool_call.call_id.clone(),
                            tool_call: Some(call),
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
