use anyhow::Result;
use forge_domain::{
    AttemptCompletionInput, ExecutableTool, NamedTool, ToolCallContext, ToolDescription, ToolOutput,
};
use forge_tool_macros::ToolDescription;

/// After each tool use, the user will respond with the result of
/// that tool use, i.e. if it succeeded or failed, along with any reasons for
/// failure. Once you've received the results of tool uses and can confirm that
/// the task is complete, use this tool to present the result of your work to
/// the user. The user may respond with feedback if they are not satisfied with
/// the result, which you can use to make improvements and try again.
/// IMPORTANT NOTE: This tool CANNOT be used until you've confirmed from the
/// user that any previous tool uses were successful. Failure to do so will
/// result in code corruption and system failure. Before using this tool, you
/// must ask yourself in <forge_thinking></forge_thinking> tags if you've
/// confirmed from the user that any previous tool uses were successful. If not,
/// then DO NOT use this tool.
#[derive(Debug, Default, ToolDescription)]
pub struct Completion;

impl NamedTool for Completion {
    fn tool_name() -> forge_domain::ToolName {
        forge_domain::ToolName::new("forge_tool_attempt_completion")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for Completion {
    type Input = AttemptCompletionInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> Result<ToolOutput> {
        // Log the completion event
        context.send_summary(input.result.clone()).await?;

        // Set the completion flag to true
        context.set_complete().await;

        // Return success with the message
        Ok(ToolOutput::text(input.result))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::utils::ToolContentExtension;

    #[tokio::test]
    async fn test_attempt_completion() {
        // Create fixture
        let tool = Completion;
        let input = AttemptCompletionInput {
            result: "All required features implemented".to_string(),
            explanation: None,
        };

        // Execute the fixture
        let actual = tool
            .call(ToolCallContext::default(), input)
            .await
            .unwrap()
            .into_string();

        // Define expected result
        let expected = "All required features implemented";

        // Assert that the actual result matches the expected result
        assert_eq!(actual, expected);
    }
}
