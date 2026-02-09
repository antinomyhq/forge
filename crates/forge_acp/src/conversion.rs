//! ACP Protocol Conversion Utilities
//!
//! This module contains pure conversion functions between ACP protocol types
//! and Forge domain types. These functions have no service dependencies and
//! are easily testable.

use std::path::PathBuf;

use agent_client_protocol as acp;
use forge_domain::{Agent, AgentId, Attachment, AttachmentContent, Image, ToolCallFull, ToolName, ToolOutput, ToolValue};

use crate::Error;

/// Converts a Forge Agent to an ACP SessionMode
///
/// # Arguments
///
/// * `agent` - The Forge agent to convert
pub fn agent_to_session_mode(agent: &Agent) -> acp::SessionMode {
    let id = acp::SessionModeId::new(agent.id.as_str().to_string());
    // Title can be too big - it will not be a good UX to show title as name.
    let name = agent.id.to_string();
    let description = agent.description.clone();

    acp::SessionMode::new(id, name).description(description)
}

/// Builds an ACP SessionModeState from available agents
///
/// # Arguments
///
/// * `agents` - List of available agents
/// * `current_agent_id` - The currently active agent ID
pub fn build_session_mode_state(
    agents: &[Agent],
    current_agent_id: &AgentId,
) -> acp::SessionModeState {
    // Convert agents to session modes
    let available_modes: Vec<acp::SessionMode> =
        agents.iter().map(agent_to_session_mode).collect();

    // Create the mode state with current agent as active
    let current_mode_id = acp::SessionModeId::new(current_agent_id.as_str().to_string());

    acp::SessionModeState::new(current_mode_id, available_modes)
}

/// Maps a Forge tool name to an ACP ToolKind
///
/// # Arguments
///
/// * `tool_name` - The Forge tool name
pub fn map_tool_kind(tool_name: &ToolName) -> acp::ToolKind {
    match tool_name.as_str() {
        "read" => acp::ToolKind::Read,
        "write" | "patch" => acp::ToolKind::Edit,
        "remove" | "undo" => acp::ToolKind::Delete,
        "fs_search" | "sem_search" => acp::ToolKind::Search,
        "shell" => acp::ToolKind::Execute,
        "fetch" => acp::ToolKind::Fetch,
        "sage" => acp::ToolKind::Think, // Research agent
        _ => {
            // Check MCP tool patterns
            let name = tool_name.as_str();
            if name.starts_with("mcp_") {
                if name.contains("read")
                    || name.contains("get")
                    || name.contains("fetch")
                    || name.contains("list")
                    || name.contains("show")
                    || name.contains("view")
                    || name.contains("load")
                {
                    acp::ToolKind::Read
                } else if name.contains("search")
                    || name.contains("query")
                    || name.contains("find")
                    || name.contains("filter")
                    || name.contains("lookup")
                {
                    acp::ToolKind::Search
                } else if name.contains("write")
                    || name.contains("update")
                    || name.contains("create")
                    || name.contains("set")
                    || name.contains("add")
                    || name.contains("insert")
                    || name.contains("push")
                    || name.contains("merge")
                    || name.contains("fork")
                    || name.contains("comment")
                    || name.contains("assign")
                    || name.contains("request")
                {
                    acp::ToolKind::Edit
                } else if name.contains("delete")
                    || name.contains("remove")
                    || name.contains("drop")
                    || name.contains("clear")
                    || name.contains("close")
                    || name.contains("cancel")
                {
                    acp::ToolKind::Delete
                } else if name.contains("execute")
                    || name.contains("run")
                    || name.contains("start")
                    || name.contains("invoke")
                    || name.contains("call")
                {
                    acp::ToolKind::Execute
                } else {
                    acp::ToolKind::Other
                }
            } else {
                acp::ToolKind::Other
            }
        }
    }
}

/// Extracts file locations from tool arguments for "follow-along" features
///
/// # Arguments
///
/// * `tool_name` - The tool name
/// * `arguments` - The tool arguments as JSON
pub fn extract_file_locations(
    tool_name: &ToolName,
    arguments: &serde_json::Value,
) -> Vec<acp::ToolCallLocation> {
    match tool_name.as_str() {
        "read" | "write" | "patch" | "remove" | "undo" => {
            if let Some(file_path) = arguments.get("file_path").and_then(|v| v.as_str()) {
                vec![acp::ToolCallLocation::new(PathBuf::from(file_path))]
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

/// Maps a Forge ToolCallFull to an ACP ToolCall
///
/// # Arguments
///
/// * `tool_call` - The Forge tool call to convert
pub fn map_tool_call_to_acp(tool_call: &ToolCallFull) -> acp::ToolCall {
    let tool_call_id = tool_call
        .call_id
        .as_ref()
        .map(|id| id.as_str().to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let title = tool_call.name.as_str().to_string();
    let kind = map_tool_kind(&tool_call.name);
    let locations = extract_file_locations(
        &tool_call.name,
        &serde_json::to_value(&tool_call.arguments).unwrap_or(serde_json::json!({})),
    );

    acp::ToolCall::new(tool_call_id, title)
        .kind(kind)
        .status(acp::ToolCallStatus::Pending)
        .locations(locations)
        .raw_input(
            serde_json::to_value(&tool_call.arguments)
                .ok()
                .filter(|v| !v.is_null()),
        )
}

/// Maps a Forge ToolOutput to ACP ToolCallContent
///
/// # Arguments
///
/// * `output` - The Forge tool output to convert
pub fn map_tool_output_to_content(output: &ToolOutput) -> Vec<acp::ToolCallContent> {
    // Check if there's a FileDiff - if so, only show that and skip text diffs
    let has_file_diff = output
        .values
        .iter()
        .any(|v| matches!(v, ToolValue::FileDiff(_)));

    output
        .values
        .iter()
        .filter_map(|value| {
            // Use display_value() to get the user-friendly representation
            let display = value.display_value();

            match display {
                ToolValue::Text(text) => {
                    // Skip text content if we have a FileDiff (text is the formatted diff for
                    // CLI)
                    if has_file_diff {
                        None
                    } else {
                        Some(acp::ToolCallContent::Content(acp::Content::new(
                            acp::ContentBlock::Text(acp::TextContent::new(text.clone())),
                        )))
                    }
                }
                ToolValue::Markdown(md) => {
                    // Markdown is for display, send as text content
                    Some(acp::ToolCallContent::Content(acp::Content::new(
                        acp::ContentBlock::Text(acp::TextContent::new(md.clone())),
                    )))
                }
                ToolValue::Image(image) => Some(acp::ToolCallContent::Content(
                    acp::Content::new(acp::ContentBlock::Image(acp::ImageContent::new(
                        image.data(),
                        image.mime_type(),
                    ))),
                )),
                ToolValue::AI { value, .. } => {
                    Some(acp::ToolCallContent::Content(acp::Content::new(
                        acp::ContentBlock::Text(acp::TextContent::new(value.clone())),
                    )))
                }
                ToolValue::FileDiff(file_diff) => {
                    // Convert Forge FileDiff to ACP Diff
                    Some(acp::ToolCallContent::Diff(
                        acp::Diff::new(PathBuf::from(&file_diff.path), &file_diff.new_text)
                            .old_text(file_diff.old_text.clone()),
                    ))
                }
                ToolValue::Empty => None,
                ToolValue::Pair(_, _) => {
                    // This shouldn't happen since display_value() unwraps pairs
                    // But handle it just in case by recursing
                    None
                }
            }
        })
        .collect()
}

/// Converts an ACP URI to a file path
///
/// Handles file:// URIs and converts them to absolute paths.
///
/// # Arguments
///
/// * `uri` - The URI to convert
pub fn uri_to_path(uri: &str) -> String {
    // Handle file:// URIs
    if let Some(path) = uri.strip_prefix("file://") {
        // Remove any leading slash for Windows paths (file:///C:/path -> C:/path)
        if path.len() > 2 && path.chars().nth(2) == Some(':') {
            path.trim_start_matches('/').to_string()
        } else {
            path.to_string()
        }
    } else {
        // Return as-is if not a file:// URI
        uri.to_string()
    }
}

/// Converts an ACP EmbeddedResource to a Forge Attachment
///
/// # Arguments
///
/// * `resource` - The ACP embedded resource to convert
///
/// # Errors
///
/// Returns an error if the resource cannot be converted.
pub fn acp_resource_to_attachment(resource: &acp::EmbeddedResource) -> Result<Attachment, Error> {
    match &resource.resource {
        acp::EmbeddedResourceResource::TextResourceContents(text) => {
            let content = AttachmentContent::FileContent {
                content: text.text.clone(),
                start_line: 1,
                end_line: text.text.lines().count() as u64,
                total_lines: text.text.lines().count() as u64,
            };
            let path = uri_to_path(&text.uri);
            Ok(Attachment { content, path })
        }
        acp::EmbeddedResourceResource::BlobResourceContents(blob) => {
            // Blob is base64 encoded
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &blob.blob)
                    .map_err(|e| Error::Application(anyhow::anyhow!("Invalid base64: {}", e)))?;

            let mime_type = blob
                .mime_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let content = AttachmentContent::Image(Image::new_bytes(bytes, mime_type));
            let path = uri_to_path(&blob.uri);
            Ok(Attachment { content, path })
        }
        _ => {
            // Handle unknown resource types
            Err(Error::Application(anyhow::anyhow!(
                "Unsupported resource type"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_uri_to_path_file_uri() {
        let fixture = "file:///home/user/file.txt";
        let actual = uri_to_path(fixture);
        let expected = "/home/user/file.txt";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_uri_to_path_windows() {
        let fixture = "file:///C:/Users/user/file.txt";
        let actual = uri_to_path(fixture);
        let expected = "C:/Users/user/file.txt";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_uri_to_path_plain() {
        let fixture = "/home/user/file.txt";
        let actual = uri_to_path(fixture);
        let expected = "/home/user/file.txt";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_map_tool_kind_builtin() {
        let fixture = ToolName::new("read");
        let actual = map_tool_kind(&fixture);
        let expected = acp::ToolKind::Read;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_map_tool_kind_mcp() {
        let fixture = ToolName::new("mcp_gh_tool_read_file");
        let actual = map_tool_kind(&fixture);
        let expected = acp::ToolKind::Read;
        assert_eq!(actual, expected);
    }
}
