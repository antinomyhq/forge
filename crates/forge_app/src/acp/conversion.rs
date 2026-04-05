use std::path::PathBuf;

use agent_client_protocol as acp;
use forge_domain::{
    Agent, AgentId, Attachment, AttachmentContent, FileInfo, ToolCallFull, ToolName, ToolOutput,
    ToolValue,
};

use super::error::{Error, Result};

pub(crate) fn map_tool_kind(tool_name: &ToolName) -> acp::ToolKind {
    match tool_name.as_str() {
        "read" => acp::ToolKind::Read,
        "write" | "patch" => acp::ToolKind::Edit,
        "remove" | "undo" => acp::ToolKind::Delete,
        "fs_search" | "sem_search" => acp::ToolKind::Search,
        "shell" => acp::ToolKind::Execute,
        "fetch" => acp::ToolKind::Fetch,
        "sage" => acp::ToolKind::Think,
        _ => {
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

pub(crate) fn extract_file_locations(
    tool_name: &ToolName,
    arguments: &serde_json::Value,
) -> Vec<acp::ToolCallLocation> {
    match tool_name.as_str() {
        "read" | "write" | "patch" | "remove" | "undo" => arguments
            .get("file_path")
            .and_then(|value| value.as_str())
            .map(|file_path| vec![acp::ToolCallLocation::new(PathBuf::from(file_path))])
            .unwrap_or_default(),
        _ => vec![],
    }
}

pub(crate) fn map_tool_call_to_acp(tool_call: &ToolCallFull) -> acp::ToolCall {
    let tool_call_id = tool_call
        .call_id
        .as_ref()
        .map(|id| id.as_str().to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let locations = extract_file_locations(
        &tool_call.name,
        &serde_json::to_value(&tool_call.arguments).unwrap_or(serde_json::json!({})),
    );

    acp::ToolCall::new(tool_call_id, tool_call.name.as_str().to_string())
        .kind(map_tool_kind(&tool_call.name))
        .status(acp::ToolCallStatus::Pending)
        .locations(locations)
        .raw_input(
            serde_json::to_value(&tool_call.arguments)
                .ok()
                .filter(|value| !value.is_null()),
        )
}

pub(crate) struct ToolOutputConverter {
    _private: (),
}

impl ToolOutputConverter {
    pub(crate) fn new(output: &ToolOutput) -> Self {
        let _ = output;
        Self { _private: () }
    }

    pub(crate) fn convert(output: &ToolOutput) -> Vec<acp::ToolCallContent> {
        let converter = Self::new(output);
        output
            .values
            .iter()
            .filter_map(|value| converter.convert_value(value))
            .collect()
    }

    fn convert_value(&self, value: &ToolValue) -> Option<acp::ToolCallContent> {
        match value {
            ToolValue::Text(text) => self.convert_text(text),
            ToolValue::AI { value, .. } => self.convert_text(value),
            ToolValue::Image(image) => Some(acp::ToolCallContent::Content(acp::Content::new(
                acp::ContentBlock::Image(acp::ImageContent::new(image.data(), image.mime_type())),
            ))),
            ToolValue::Empty => None,
        }
    }

    fn convert_text(&self, text: &str) -> Option<acp::ToolCallContent> {
        if text.is_empty() {
            None
        } else {
            Some(acp::ToolCallContent::Content(acp::Content::new(
                acp::ContentBlock::Text(acp::TextContent::new(text.to_string())),
            )))
        }
    }
}

pub(crate) fn acp_resource_to_attachment(resource: &acp::EmbeddedResource) -> Result<Attachment> {
    let (content_text, uri) = match &resource.resource {
        acp::EmbeddedResourceResource::TextResourceContents(text_resource) => {
            (text_resource.text.clone(), text_resource.uri.clone())
        }
        acp::EmbeddedResourceResource::BlobResourceContents(blob_resource) => {
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &blob_resource.blob,
            )
            .map_err(|error| {
                Error::Application(anyhow::anyhow!("Failed to decode base64 blob: {}", error))
            })?;
            let text = String::from_utf8(decoded).map_err(|error| {
                Error::Application(anyhow::anyhow!("Failed to decode UTF-8: {}", error))
            })?;
            (text, blob_resource.uri.clone())
        }
        _ => {
            return Err(Error::Application(anyhow::anyhow!(
                "Unsupported resource type"
            )))
        }
    };

    let path = uri_to_path(&uri);
    let total_lines = content_text.lines().count() as u64;
    let info = FileInfo::new(1, total_lines, total_lines, String::new());
    let content = AttachmentContent::FileContent {
        content: content_text,
        info,
    };

    Ok(Attachment { path, content })
}

pub(crate) fn uri_to_path(uri: &str) -> String {
    if let Some(path) = uri.strip_prefix("file://") {
        if path.len() > 2 && path.chars().nth(2) == Some(':') {
            path.trim_start_matches('/').to_string()
        } else {
            path.to_string()
        }
    } else {
        uri.to_string()
    }
}

pub(crate) fn build_session_mode_state(
    agents: &[Agent],
    current_agent_id: &AgentId,
) -> acp::SessionModeState {
    let available_modes = agents
        .iter()
        .map(|agent| {
            acp::SessionMode::new(
                acp::SessionModeId::new(agent.id.to_string()),
                agent.id.to_string(),
            )
            .description(agent.description.clone())
        })
        .collect();

    acp::SessionModeState::new(
        acp::SessionModeId::new(current_agent_id.to_string()),
        available_modes,
    )
}

#[cfg(test)]
mod tests {
    use forge_domain::{ConversationId, Image};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_uri_to_path_preserves_non_file_uri() {
        let fixture = "relative/path.txt";
        let actual = uri_to_path(fixture);
        let expected = "relative/path.txt".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_markdown_sent_to_acp_not_xml() {
        let fixture = ToolOutput::text("## File: test.txt\n\nContent here");

        let actual = ToolOutputConverter::convert(&fixture);

        assert_eq!(actual.len(), 1);
        if let Some(acp::ToolCallContent::Content(content)) = actual.first() {
            if let acp::ContentBlock::Text(text) = &content.content {
                assert_eq!(text.text, "## File: test.txt\n\nContent here");
            } else {
                panic!("Expected text content block");
            }
        } else {
            panic!("Expected content");
        }
    }

    #[test]
    fn test_ai_output_sent_to_acp_as_text() {
        let fixture = ToolOutput::ai(ConversationId::generate(), "Agent result");

        let actual = ToolOutputConverter::convert(&fixture);

        assert_eq!(actual.len(), 1);
        if let Some(acp::ToolCallContent::Content(content)) = actual.first() {
            if let acp::ContentBlock::Text(text) = &content.content {
                assert_eq!(text.text, "Agent result");
            } else {
                panic!("Expected text content block");
            }
        } else {
            panic!("Expected content");
        }
    }

    #[test]
    fn test_image_sent_to_acp() {
        let image = Image::new_bytes(vec![1, 2, 3, 4], "image/png".to_string());
        let fixture = ToolOutput::image(image);

        let actual = ToolOutputConverter::convert(&fixture);

        assert_eq!(actual.len(), 1);
        if let Some(acp::ToolCallContent::Content(content)) = actual.first() {
            assert!(matches!(content.content, acp::ContentBlock::Image(_)));
        } else {
            panic!("Expected content");
        }
    }
}