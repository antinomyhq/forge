use forge_domain::{ContextSummary, Conversation, SummaryBlock};
use serde::Serialize;

use crate::template_engine::TemplateEngine;

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

/// Data structure for rendering conversation as markdown
#[derive(Debug, Serialize)]
struct ConversationMarkdownData {
    frontmatter: ConversationFrontmatter,
    messages: Vec<SummaryBlock>,
}

impl From<&Conversation> for ConversationMarkdownData {
    fn from(conversation: &Conversation) -> Self {
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

        let messages = conversation
            .context
            .as_ref()
            .map(|ctx| ContextSummary::from(ctx).messages)
            .unwrap_or_default();

        Self { frontmatter, messages }
    }
}

/// Renders a conversation as markdown with YAML frontmatter using templates
///
/// # Errors
/// Returns an error if serialization or template rendering fails
pub fn render_conversation_markdown(conversation: &Conversation) -> anyhow::Result<String> {
    let data = ConversationMarkdownData::from(conversation);

    // Render the markdown content using the template
    let content = if !data.messages.is_empty() {
        TemplateEngine::default().render(
            "forge-partial-summary-frame.md",
            &serde_json::json!({"messages": data.messages}),
        )?
    } else {
        String::new()
    };

    // Manually construct frontmatter with content
    let frontmatter_yaml = serde_yml::to_string(&data.frontmatter)?;
    let full_content = format!("---\n{}---\n\n{}", frontmatter_yaml, content);

    Ok(full_content)
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ContextMessage, Conversation, Role, TextMessage};

    use super::*;

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
        assert!(result.contains("Hello, how are you?"));
        assert!(result.contains("I'm doing well, thank you!"));
        assert!(result.contains("message_count: 2"));
    }

    #[test]
    fn test_frontmatter_includes_file_operations() {
        let conversation = Conversation::generate().metrics(
            forge_domain::Metrics::default().insert(
                "test.rs".to_string(),
                forge_domain::FileOperation::new(forge_domain::ToolKind::Write)
                    .lines_added(10u64)
                    .lines_removed(5u64),
            ),
        );

        let result = render_conversation_markdown(&conversation).unwrap();

        assert!(result.contains("file_operations_count: 1"));
    }

    #[test]
    fn test_conversation_markdown_data_conversion() {
        let conversation =
            Conversation::generate()
                .title(Some("Test".to_string()))
                .context(Context::default().messages(vec![ContextMessage::Text(
                    TextMessage::new(Role::User, "Hello"),
                )]));

        let data = ConversationMarkdownData::from(&conversation);

        assert_eq!(data.frontmatter.title, Some("Test".to_string()));
        assert_eq!(data.frontmatter.message_count, 1);
        assert_eq!(data.messages.len(), 1);
    }
}
