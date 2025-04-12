use forge_domain::{ExecutableTool, NamedTool, ToolCallContext, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This tool allows the agent to communicate information to the user with
/// proper text formatting. Use this when you need to show a message or ask for
/// a clarification to the user. The content parameter must contain valid
/// markdown syntax. Returns a simple confirmation string but does not capture
/// user responses.
#[derive(Clone, Default, ToolDescription)]
pub struct ShowUser;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShowUserInput {
    /// The markdown content to display to the user. Should contain valid
    /// markdown syntax such as headers (#), lists (-, *), emphasis
    /// (**bold**, *italic*), code blocks, and other markdown formatting
    /// elements.
    pub content: String,
}

impl NamedTool for ShowUser {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_display_show_user")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for ShowUser {
    type Input = ShowUserInput;
    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        // Use termimad to display the markdown to the terminal

        let skin = termimad::get_default_skin();
        let content = skin.term_text(&input.content);
        context.send_text(content.to_string()).await?;

        // Return a simple success message
        Ok("Markdown content displayed to user".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_show_user() {
        let show_user = ShowUser;
        let input = ShowUserInput {
            content: "# Test Heading\nThis is a test with **bold** and *italic* text.".to_string(),
        };

        // Create a test context
        let context = ToolCallContext::default();

        // The function should execute without error and return a success message
        let result = show_user.call(context, input).await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "Markdown content displayed to user".to_string()
        );
    }
}
