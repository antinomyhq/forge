//! Type conversions between ACP protocol types and Forge domain types

use agent_client_protocol as acp;
use forge_domain::{Agent, AgentId, Attachment, ToolCallFull, ToolOutput};

use super::error::{Error, Result};

/// Converts a Forge ToolCallFull to an ACP ToolCallUpdate
pub(crate) fn map_tool_call_to_acp(tool_call: &ToolCallFull) -> acp::ToolCallUpdate {
    let tool_call_id = tool_call
        .call_id
        .as_ref()
        .map(|id| id.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let mut fields = acp::ToolCallUpdateFields::new()
        .title(tool_call.name.to_string())
        .status(acp::ToolCallStatus::InProgress);

    // Set tool kind based on the tool name
    let kind = match tool_call.name.as_str() {
        name if name.contains("search") || name.contains("query") => acp::ToolKind::Search,
        name if name.contains("read") || name.contains("write") || name.contains("edit") => {
            acp::ToolKind::Edit
        }
        name if name.contains("shell") || name.contains("command") => acp::ToolKind::Execute,
        _ => acp::ToolKind::Think,
    };

    fields = fields.kind(kind);

    acp::ToolCallUpdate::new(tool_call_id, fields)
}

/// Converts a Forge ToolOutput to ACP ToolCallContent
pub(crate) fn map_tool_output_to_content(output: &ToolOutput) -> Vec<acp::ToolCallContent> {
    use forge_domain::ToolValue;

    let mut content = Vec::new();

    // Convert each ToolValue to ACP content
    for value in &output.values {
        match value {
            ToolValue::Text(text) if !text.is_empty() => {
                content.push(acp::ToolCallContent::Content(acp::Content::new(
                    acp::ContentBlock::Text(acp::TextContent::new(text.clone())),
                )));
            }
            ToolValue::AI { value, .. } if !value.is_empty() => {
                content.push(acp::ToolCallContent::Content(acp::Content::new(
                    acp::ContentBlock::Text(acp::TextContent::new(value.clone())),
                )));
            }
            ToolValue::Image(img) => {
                // Convert image to ACP format if needed
                // For now, just add as text reference
                content.push(acp::ToolCallContent::Content(acp::Content::new(
                    acp::ContentBlock::Text(acp::TextContent::new(format!(
                        "[Image: {}]",
                        img.mime_type()
                    ))),
                )));
            }
            ToolValue::Empty => {
                // Skip empty values
            }
            _ => {
                // Skip other value types or empty text
            }
        }
    }

    content
}

/// Converts an ACP embedded resource to a Forge Attachment
pub(crate) fn acp_resource_to_attachment(resource: &acp::EmbeddedResource) -> Result<Attachment> {
    use forge_domain::AttachmentContent;

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
pub(crate) fn uri_to_path(uri: &str) -> String {
    // Handle file:// URIs
    if let Some(path) = uri.strip_prefix("file://") {
        return path.to_string();
    }

    // Handle other schemes by extracting the path component
    if let Some(idx) = uri.find("://") {
        return uri[idx + 3..].to_string();
    }

    // Return as-is if no scheme
    uri.to_string()
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
            let mut mode_info = acp::SessionMode::new(mode_id, agent.title.clone());

            // Add description if available
            if let Some(desc) = &agent.description {
                mode_info = mode_info.description(desc.clone());
            }

            mode_info
        })
        .collect();

    acp::SessionModeState::new(
        acp::SessionModeId::new(current_agent_id.to_string()),
        available_modes,
    )
}
