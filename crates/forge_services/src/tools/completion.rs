use anyhow::Result;
use forge_domain::{ExecutableTool, NamedTool, ToolCallContext, ToolDescription};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

/// Once you can confirm that the task is complete, use this tool to present the
/// result of your work to the user. The user may respond with feedback if they
/// are not satisfied with the result, which you can use to make improvements
/// and try again.
#[derive(Debug, Default, ToolDescription)]
pub struct Completion;

impl NamedTool for Completion {
    fn tool_name() -> forge_domain::ToolName {
        forge_domain::ToolName::new("forge_tool_attempt_completion")
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct AttemptCompletionInput {
    /// Summary message describing the completed task
    message: String,
}

#[async_trait::async_trait]
impl ExecutableTool for Completion {
    type Input = AttemptCompletionInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> Result<String> {
        // Log the completion event
        context.send_summary(input.message.clone()).await?;

        // Set the completion flag to true
        context.set_complete().await;

        // Return success with the message
        Ok(input.message)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_attempt_completion() {
        // Create fixture
        let tool = Completion;
        let input =
            AttemptCompletionInput { message: "All required features implemented".to_string() };

        // Execute the fixture
        let actual = tool.call(ToolCallContext::default(), input).await.unwrap();

        // Define expected result
        let expected = "All required features implemented";

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }
}
