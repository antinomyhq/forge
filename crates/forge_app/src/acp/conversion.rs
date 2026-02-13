//! Type conversions between ACP protocol types and Forge domain types

use agent_client_protocol as acp;
use forge_domain::{Agent, AgentId, Attachment, AttachmentContent, ToolCallFull, ToolName, ToolOutput, ToolValue};
use std::path::PathBuf;

use super::error::{Error, Result};

/// Maps a Forge tool name to an ACP ToolKind
///
/// # Arguments
///
/// * `tool_name` - The Forge tool name
pub(crate) fn map_tool_kind(tool_name: &ToolName) -> acp::ToolKind {
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
pub(crate) fn extract_file_locations(
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

/// Converts a Forge ToolCallFull to an ACP ToolCall
pub(crate) fn map_tool_call_to_acp(tool_call: &ToolCallFull) -> acp::ToolCall {
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

/// Converter for transforming Forge ToolOutput to ACP ToolCallContent
///
/// Handles the conversion of tool output values to their ACP protocol representations,
/// with special logic for display values and file diffs.
pub(crate) struct ToolOutputConverter {
    has_file_diff: bool,
}

impl ToolOutputConverter {
    /// Creates a new converter for the given tool output
    ///
    /// # Arguments
    ///
    /// * `output` - The tool output to analyze
    pub(crate) fn new(output: &ToolOutput) -> Self {
        let has_file_diff = output
            .values
            .iter()
            .any(|v| matches!(v.display_value(), forge_domain::ToolValue::FileDiff(_)));

        Self { has_file_diff }
    }

    /// Converts all values in the tool output to ACP content
    ///
    /// # Arguments
    ///
    /// * `output` - The tool output to convert
    pub(crate) fn convert(output: &ToolOutput) -> Vec<acp::ToolCallContent> {
        let converter = Self::new(output);
        output
            .values
            .iter()
            .filter_map(|value| converter.convert_value(value))
            .collect()
    }

    /// Converts a single ToolValue to ACP ToolCallContent
    fn convert_value(&self, value: &forge_domain::ToolValue) -> Option<acp::ToolCallContent> {
        let display = value.display_value();

        match display {
            ToolValue::Text(text) => self.convert_text(text),
            ToolValue::AI { value, .. } => self.convert_ai_text(value),
            ToolValue::Markdown(md) => self.convert_markdown(md),
            ToolValue::Image(image) => self.convert_image(image),
            ToolValue::FileDiff(file_diff) => self.convert_file_diff(file_diff),
            ToolValue::Pair(_, _) => None, // Already unwrapped by display_value()
            ToolValue::Empty => None,
        }
    }

    fn convert_text(&self, text: &str) -> Option<acp::ToolCallContent> {
        // Skip text if we have a FileDiff (text is CLI-formatted diff)
        if self.has_file_diff || text.is_empty() {
            None
        } else {
            Some(Self::text_content(text))
        }
    }

    fn convert_ai_text(&self, text: &str) -> Option<acp::ToolCallContent> {
        (!text.is_empty()).then(|| Self::text_content(text))
    }

    fn convert_markdown(&self, markdown: &str) -> Option<acp::ToolCallContent> {
        (!markdown.is_empty()).then(|| Self::text_content(markdown))
    }

    fn convert_image(&self, image: &forge_domain::Image) -> Option<acp::ToolCallContent> {
        Some(acp::ToolCallContent::Content(acp::Content::new(
            acp::ContentBlock::Image(acp::ImageContent::new(image.data(), image.mime_type())),
        )))
    }

    fn convert_file_diff(
        &self,
        file_diff: &forge_domain::FileDiff,
    ) -> Option<acp::ToolCallContent> {
        Some(acp::ToolCallContent::Diff(
            acp::Diff::new(PathBuf::from(&file_diff.path), &file_diff.new_text)
                .old_text(file_diff.old_text.clone()),
        ))
    }

    fn text_content(text: &str) -> acp::ToolCallContent {
        acp::ToolCallContent::Content(acp::Content::new(acp::ContentBlock::Text(
            acp::TextContent::new(text.to_string()),
        )))
    }
}

/// Converts an ACP embedded resource to a Forge Attachment
pub(crate) fn acp_resource_to_attachment(resource: &acp::EmbeddedResource) -> Result<Attachment> {
    // Get the text content and URI from the resource
    let (content_text, uri) = match &resource.resource {
        acp::EmbeddedResourceResource::TextResourceContents(text_res) => {
            (text_res.text.clone(), text_res.uri.clone())
        }
        acp::EmbeddedResourceResource::BlobResourceContents(blob_res) => {
            // Decode base64 blob
            let decoded =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &blob_res.blob)
                    .map_err(|e| {
                        Error::Application(anyhow::anyhow!("Failed to decode base64 blob: {}", e))
                    })?;
            let text = String::from_utf8(decoded).map_err(|e| {
                Error::Application(anyhow::anyhow!("Failed to decode UTF-8: {}", e))
            })?;
            (text, blob_res.uri.clone())
        }
        _ => {
            return Err(Error::Application(anyhow::anyhow!(
                "Unsupported resource type"
            )))
        }
    };

    // Extract path from URI
    let path = uri_to_path(&uri);

    // Create file content attachment
    // We don't know the exact line numbers from ACP, so use defaults
    let content = AttachmentContent::FileContent {
        content: content_text.clone(),
        start_line: 1,
        end_line: content_text.lines().count() as u64,
        total_lines: content_text.lines().count() as u64,
    };

    Ok(Attachment { path: path.to_string(), content })
}

/// Converts a URI to a file path
///
/// Handles file:// URIs and converts them to absolute paths.
/// Properly handles Windows paths (file:///C:/path -> C:/path).
pub(crate) fn uri_to_path(uri: &str) -> String {
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

/// Builds the SessionModeState from available agents
pub(crate) fn build_session_mode_state(
    agents: &[Agent],
    current_agent_id: &AgentId,
) -> acp::SessionModeState {
    let available_modes: Vec<acp::SessionMode> = agents
        .iter()
        .map(|agent| {
            let mode_id = acp::SessionModeId::new(agent.id.to_string());
            // Use agent ID as name (not title) for better UX
            // Title can be too long and not suitable for dropdown display
            let mode_info = acp::SessionMode::new(
                mode_id,
                agent.id.to_string(),
            ).description(agent.description.clone());

            mode_info
        })
        .collect();

    acp::SessionModeState::new(
        acp::SessionModeId::new(current_agent_id.to_string()),
        available_modes,
    )
}
