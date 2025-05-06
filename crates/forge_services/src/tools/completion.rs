use anyhow::Result;
use forge_domain::{ExecutableTool, NamedTool, ToolCallContext, ToolDescription};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

/// After each tool use, the user will respond with the result of
/// that tool use, i.e. if it succeeded or failed, along with any reasons for
/// failure. Once you've received the results of tool uses and can confirm that
/// the task is complete, use this tool to present the result of your work to
/// the user. Optionally you may provide a CLI command to showcase the result of
/// your work. The user may respond with feedback if they are not satisfied with
/// the result, which you can use to make improvements and try again.
/// IMPORTANT NOTE: This tool CANNOT be used until you've confirmed from the
/// user that any previous tool uses were successful. Failure to do so will
/// result in code corruption and system failure. Before using this tool, you
/// must ask yourself in <thinking></thinking> tags if you've confirmed from the
/// user that any previous tool uses were successful. If not, then DO NOT use
/// this tool.

#[derive(Debug, Default, ToolDescription)]
pub struct Completion;

impl NamedTool for Completion {
    fn tool_name() -> forge_domain::ToolName {
        forge_domain::ToolName::new("forge_tool_attempt_completion")
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct AttemptCompletionInput {
    /// The result of the task. Formulate this result in a way that is final and
    /// does not require further input from the user. Don't end your result with
    /// questions or offers for further assistance.
    result: String,
}

#[async_trait::async_trait]
impl ExecutableTool for Completion {
    type Input = AttemptCompletionInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> Result<String> {
        // Log the completion event
        context.send_summary(input.result.clone()).await?;

        // Set the completion flag to true
        context.set_complete().await;

        // Return success with the message
        Ok(input.result)
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
            AttemptCompletionInput { result: "All required features implemented".to_string() };

        // Execute the fixture
        let actual = tool.call(ToolCallContext::default(), input).await.unwrap();

        // Define expected result
        let expected = "All required features implemented";

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }
}
