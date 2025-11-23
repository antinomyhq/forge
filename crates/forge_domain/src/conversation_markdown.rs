use serde::Serialize;

use crate::{ContextMessage, Conversation, Result, Role, ToolValue};

/// Frontmatter metadata for the conversation
#[derive(Debug, Serialize)]
struct ConversationFrontmatter {
    id: String,
    title: Option<String>,
    created_at: String,
    updated_at: Option<String>,
    file_operations_count: usize,
    message_count: usize,
}

/// Renders a conversation as markdown with YAML frontmatter
///
/// # Errors
/// Returns an error if serialization fails
pub fn render_conversation_markdown(conversation: &Conversation) -> Result<String> {
    // Prepare frontmatter
    let frontmatter = ConversationFrontmatter {
        id: conversation.id.into_string(),
        title: conversation.title.clone(),
        created_at: conversation.metadata.created_at.to_rfc3339(),
        updated_at: conversation.metadata.updated_at.map(|dt| dt.to_rfc3339()),
        file_operations_count: conversation.metrics.file_operations.len(),
        message_count: conversation
            .context
            .as_ref()
            .map(|ctx| ctx.messages.len())
            .unwrap_or(0),
    };

    // Build markdown content
    let mut content = String::new();

    if let Some(context) = &conversation.context {
        for message in &context.messages {
            content.push_str(&render_message(message));
            content.push_str("\n\n");
        }
    }

    // Manually construct frontmatter with content
    let frontmatter_yaml = serde_yml::to_string(&frontmatter).map_err(|e| {
        crate::Error::from(anyhow::anyhow!("Failed to serialize frontmatter: {}", e))
    })?;

    let full_content = format!("---\n{}---\n\n{}", frontmatter_yaml, content);

    Ok(full_content)
}

/// Renders a single context message as markdown
fn render_message(message: &ContextMessage) -> String {
    match message {
        ContextMessage::Text(text_message) => {
            let role = match text_message.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
            };

            let mut output = format!("## {role}\n\n");

            // Add reasoning if present
            if let Some(reasoning_details) = &text_message.reasoning_details
                && !reasoning_details.is_empty()
            {
                output.push_str("### Reasoning\n\n");
                for reasoning in reasoning_details {
                    if let Some(text) = &reasoning.text {
                        output.push_str("```\n");
                        output.push_str(text);
                        output.push_str("\n```\n\n");
                    }
                }
            }

            // Add main content
            output.push_str(&text_message.content);

            // Add tool calls if present
            if let Some(tool_calls) = &text_message.tool_calls
                && !tool_calls.is_empty()
            {
                output.push_str("\n\n### Tool Calls\n\n");
                for tool_call in tool_calls {
                    output.push_str(&format!("**{}**\n\n", tool_call.name));
                    output.push_str("```json\n");
                    output.push_str(&tool_call.arguments.clone().into_string());
                    output.push_str("\n```\n\n");
                }
            }

            output
        }
        ContextMessage::Tool(tool_result) => {
            let mut output = format!("## Tool Result: {}\n\n", tool_result.name);

            if tool_result.output.is_error {
                output.push_str("**Error:**\n\n");
            }

            output.push_str("```xml\n");
            for value in &tool_result.output.values {
                match value {
                    ToolValue::Text(text) => {
                        output.push_str(text);
                        if !text.ends_with('\n') {
                            output.push('\n');
                        }
                    }
                    ToolValue::Image(image) => {
                        if image.url().starts_with("data:") {
                            output.push_str(&format!("[Base64 Image: {}]\n", image.mime_type()));
                        } else {
                            output.push_str(&format!("![Image]({})\n", image.url()));
                        }
                    }
                    ToolValue::Empty => {
                        // Empty value, skip
                    }
                }
            }
            output.push_str("```\n");

            output
        }
        ContextMessage::Image(image) => {
            if image.url().starts_with("data:") {
                format!("## Image\n\n[Base64 Image: {}]\n", image.mime_type())
            } else {
                format!("## Image\n\n![Image]({})\n", image.url())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Context, Conversation, TextMessage};

    #[test]
    fn test_render_empty_conversation() {
        let conversation = Conversation::generate();
        let result = render_conversation_markdown(&conversation).unwrap();

        assert!(result.contains("---"));
        assert!(result.contains("id:"));
        assert!(result.contains("created_at:"));
        assert!(result.contains("message_count: 0"));
    }

    #[test]
    fn test_render_conversation_with_messages() {
        let conversation = Conversation::generate()
            .title(Some("Test Conversation".to_string()))
            .context(Context::default().messages(vec![
                ContextMessage::Text(TextMessage::new(Role::User, "Hello, how are you?")),
                ContextMessage::Text(TextMessage::new(
                    Role::Assistant,
                    "I'm doing well, thank you!",
                )),
            ]));

        let result = render_conversation_markdown(&conversation).unwrap();

        assert!(result.contains("title: Test Conversation"));
        assert!(result.contains("## User"));
        assert!(result.contains("Hello, how are you?"));
        assert!(result.contains("## Assistant"));
        assert!(result.contains("I'm doing well, thank you!"));
        assert!(result.contains("message_count: 2"));
    }

    #[test]
    fn test_frontmatter_includes_file_operations() {
        let conversation = Conversation::generate().metrics(
            crate::Metrics::default().insert(
                "test.rs".to_string(),
                crate::FileOperation::new(crate::ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64),
            ),
        );

        let result = render_conversation_markdown(&conversation).unwrap();

        assert!(result.contains("file_operations_count: 1"));
    }
}
