use anyhow::Result;
use forge_display::MarkdownFormat;
use forge_domain::ContextMessage;

/// Formats a context message for display
///
/// # Arguments
/// - `message`: The message to format
/// - `markdown`: The markdown formatter to use for rendering content
///
/// # Returns
/// A vector of formatted strings ready for display
///
/// # Errors
/// - If JSON serialization fails for tool results
pub fn format_message(message: &ContextMessage, markdown: &MarkdownFormat) -> Result<Vec<String>> {
    let mut lines = Vec::new();

    match message {
        ContextMessage::Text(text_message) => {
            lines.push(format!("Role: {}", text_message.role));
            lines.push(String::new());
            lines.push(markdown.render(&text_message.content));

            // Show tool calls if present
            if let Some(tool_calls) = &text_message.tool_calls
                && !tool_calls.is_empty()
            {
                lines.push(String::new());
                lines.push("Tool Calls:".to_string());
                for tool_call in tool_calls {
                    lines.push(format!(
                        "  - {}: {}",
                        tool_call.name,
                        tool_call.arguments.clone().into_string()
                    ));
                }
            }

            // Show reasoning details if present
            if let Some(reasoning_details) = &text_message.reasoning_details
                && !reasoning_details.is_empty()
            {
                lines.push(String::new());
                lines.push("Reasoning:".to_string());
                for reasoning in reasoning_details {
                    if let Some(text) = &reasoning.text {
                        lines.push(format!("  {}", text));
                    }
                }
            }
        }
        ContextMessage::Tool(tool_result) => {
            lines.push(format!("Role: tool ({})", tool_result.name));
            lines.push(String::new());
            lines.push("Tool Result:".to_string());
            let output_json = serde_json::to_string_pretty(&tool_result.output)?;
            lines.push(output_json);
        }
        ContextMessage::Image(image) => {
            lines.push("Role: user (image)".to_string());
            lines.push(String::new());
            lines.push(format!("Image URL: {}", image.url()));
        }
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use forge_domain::{ContextMessage, ReasoningFull, ToolCallFull};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_format_text_message_basic() {
        let markdown = MarkdownFormat::new();
        let message = ContextMessage::user("Hello world", None);

        let actual = format_message(&message, &markdown).unwrap();

        assert!(actual[0].contains("Role: User"));
        assert_eq!(actual[1], "");
        assert!(actual[2].contains("Hello world"));
    }

    #[test]
    fn test_format_text_message_with_tool_calls() {
        let markdown = MarkdownFormat::new();
        let tool_call = ToolCallFull::new("read").arguments(
            forge_domain::ToolCallArguments::from_json(r#"{"path": "test.rs"}"#),
        );
        let message = ContextMessage::assistant("Running tool", None, Some(vec![tool_call]));

        let actual = format_message(&message, &markdown).unwrap();

        assert!(actual[0].contains("Role: Assistant"));
        assert!(actual.iter().any(|line| line.contains("Tool Calls:")));
        assert!(
            actual
                .iter()
                .any(|line| line.contains("read") && line.contains("test.rs"))
        );
    }

    #[test]
    fn test_format_text_message_with_reasoning() {
        let markdown = MarkdownFormat::new();
        let reasoning = ReasoningFull {
            text: Some("Thinking about the problem".to_string()),
            signature: None,
        };
        let message = ContextMessage::assistant("Answer", Some(vec![reasoning]), None);

        let actual = format_message(&message, &markdown).unwrap();

        assert!(actual[0].contains("Role: Assistant"));
        assert!(actual.iter().any(|line| line.contains("Reasoning:")));
        assert!(
            actual
                .iter()
                .any(|line| line.contains("Thinking about the problem"))
        );
    }

    #[test]
    fn test_format_image_message() {
        let markdown = MarkdownFormat::new();
        let image = forge_domain::Image::new_base64("test_data".to_string(), "image/png");
        let message = ContextMessage::Image(image);

        let actual = format_message(&message, &markdown).unwrap();

        assert!(actual[0].contains("Role: user (image)"));
        assert!(actual.iter().any(|line| line.contains("Image URL:")));
    }
}
