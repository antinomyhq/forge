use forge_domain::Transformer;
use regex::Regex;

use crate::dto::anthropic::{Content, Request};

/// Transformer that sanitizes tool call IDs for Anthropic/Vertex Anthropic
/// compatibility.
///
/// Anthropic requires tool call IDs to match the pattern `^[a-zA-Z0-9_-]+$`.
/// This transformer replaces any invalid characters (non-alphanumeric,
/// non-underscore, non-hyphen) with underscores.
///
/// This is particularly important for Vertex AI Anthropic which strictly
/// validates tool_use.id and tool_result.tool_use_id fields and will reject
/// requests with IDs containing invalid characters with a 400 Bad Request
/// error.
///
/// # Example
///
/// ```ignore
/// // Before transformation:
/// tool_use.id = "call_123@#$%^&*()"
///
/// // After transformation:
/// tool_use.id = "call_123_________"
/// ```
pub struct SanitizeToolIds;

impl Transformer for SanitizeToolIds {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        let regex = &*INVALID_CHARS;

        for message in &mut request.messages {
            for content in &mut message.content {
                match content {
                    Content::ToolUse { id, .. } => {
                        *id = regex.replace_all(id, "_").to_string();
                    }
                    Content::ToolResult { tool_use_id, .. } => {
                        *tool_use_id = regex.replace_all(tool_use_id, "_").to_string();
                    }
                    _ => {}
                }
            }
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Context, ContextMessage, ModelId, Role, TextMessage, ToolCallArguments, ToolCallFull,
        ToolCallId, ToolResult, Transformer,
    };

    use super::*;

    #[test]
    fn test_sanitizes_tool_use_id_with_invalid_chars() {
        let fixture = Context::default().messages(vec![
            ContextMessage::Text(
                TextMessage::new(Role::User, "test")
                    .tool_calls(vec![
                        ToolCallFull::new("test_tool")
                            .call_id("call_123@#$%^&*()")
                            .arguments(ToolCallArguments::from_json("{}")),
                    ])
                    .model(ModelId::new("claude-3-5-sonnet-20241022")),
            )
            .into(),
        ]);

        let mut request = Request::try_from(fixture).unwrap();
        request = SanitizeToolIds.transform(request);

        // Find the ToolUse content
        let tool_use = request.messages.iter().find_map(|msg| {
            msg.content.iter().find_map(|content| {
                if let Content::ToolUse { id, .. } = content {
                    Some(id.clone())
                } else {
                    None
                }
            })
        });

        assert_eq!(tool_use, Some("call_123_________".to_string()));
    }

    #[test]
    fn test_sanitizes_tool_result_id_with_invalid_chars() {
        let fixture = Context::default().messages(vec![
            ContextMessage::tool_result(
                ToolResult::new("test_tool")
                    .call_id(ToolCallId::new("toolu_01!@#$ABC123"))
                    .success("result"),
            )
            .into(),
        ]);

        let mut request = Request::try_from(fixture).unwrap();
        request = SanitizeToolIds.transform(request);

        // Find the ToolResult content
        let tool_result_id = request.messages.iter().find_map(|msg| {
            msg.content.iter().find_map(|content| {
                if let Content::ToolResult { tool_use_id, .. } = content {
                    Some(tool_use_id.clone())
                } else {
                    None
                }
            })
        });

        assert_eq!(tool_result_id, Some("toolu_01____ABC123".to_string()));
    }

    #[test]
    fn test_leaves_valid_tool_ids_unchanged() {
        let fixture = Context::default().messages(vec![
            ContextMessage::Text(
                TextMessage::new(Role::User, "test")
                    .tool_calls(vec![
                        ToolCallFull::new("test_tool")
                            .call_id("call_abc-123_XYZ")
                            .arguments(ToolCallArguments::from_json("{}")),
                    ])
                    .model(ModelId::new("claude-3-5-sonnet-20241022")),
            )
            .into(),
        ]);

        let mut request = Request::try_from(fixture).unwrap();
        request = SanitizeToolIds.transform(request);

        // Find the ToolUse content
        let tool_use = request.messages.iter().find_map(|msg| {
            msg.content.iter().find_map(|content| {
                if let Content::ToolUse { id, .. } = content {
                    Some(id.clone())
                } else {
                    None
                }
            })
        });

        assert_eq!(tool_use, Some("call_abc-123_XYZ".to_string()));
    }

    #[test]
    fn test_handles_multiple_tool_calls_and_results() {
        let fixture = Context::default().messages(vec![
            ContextMessage::Text(
                TextMessage::new(Role::User, "test")
                    .tool_calls(vec![
                        ToolCallFull::new("tool1")
                            .call_id("call_1@#$")
                            .arguments(ToolCallArguments::from_json("{}")),
                        ToolCallFull::new("tool2")
                            .call_id("call_2!@#")
                            .arguments(ToolCallArguments::from_json("{}")),
                    ])
                    .model(ModelId::new("claude-3-5-sonnet-20241022")),
            )
            .into(),
            ContextMessage::tool_result(
                ToolResult::new("tool1")
                    .call_id(ToolCallId::new("call_1@#$"))
                    .success("result1"),
            )
            .into(),
            ContextMessage::tool_result(
                ToolResult::new("tool2")
                    .call_id(ToolCallId::new("call_2!@#"))
                    .success("result2"),
            )
            .into(),
        ]);

        let mut request = Request::try_from(fixture).unwrap();
        request = SanitizeToolIds.transform(request);

        // Collect all tool use IDs
        let mut tool_use_ids = Vec::new();
        let mut tool_result_ids = Vec::new();

        for msg in &request.messages {
            for content in &msg.content {
                match content {
                    Content::ToolUse { id, .. } => tool_use_ids.push(id.clone()),
                    Content::ToolResult { tool_use_id, .. } => {
                        tool_result_ids.push(tool_use_id.clone())
                    }
                    _ => {}
                }
            }
        }

        assert_eq!(tool_use_ids, vec!["call_1___", "call_2___"]);
        assert_eq!(tool_result_ids, vec!["call_1___", "call_2___"]);
    }

    #[test]
    fn test_handles_empty_messages() {
        let fixture = Context::default().messages(vec![
            ContextMessage::Text(
                TextMessage::new(Role::User, "test")
                    .model(ModelId::new("claude-3-5-sonnet-20241022")),
            )
            .into(),
        ]);

        let mut request = Request::try_from(fixture).unwrap();
        request = SanitizeToolIds.transform(request);

        // Should not panic and should preserve the message
        assert_eq!(request.messages.len(), 1);
    }
}
