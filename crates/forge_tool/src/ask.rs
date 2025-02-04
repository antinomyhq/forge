use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct AskFollowUpQuestionInput {
    /// The question to ask the user. This should be a clear, specific question
    /// that addresses the information you need.
    pub question: String,
}

/// Ask the user a question to gather additional information needed to complete
/// the task. This tool should be used when you encounter ambiguities, need
/// clarification, or require more details to proceed effectively. It allows for
/// interactive problem-solving by enabling direct communication with the user.
#[derive(ToolDescription)]
pub struct AskFollowUpQuestion;

impl NamedTool for AskFollowUpQuestion {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_ui_ask")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for AskFollowUpQuestion {
    type Input = AskFollowUpQuestionInput;

    async fn call(&self, input: Self::Input) -> Result<String, String> {
        Ok(format!("Question: {}", input.question))
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_ask_followup_question() {
        let ask = AskFollowUpQuestion;
        let result = ask
            .call(AskFollowUpQuestionInput { question: "What is your favorite color?".to_string() })
            .await
            .unwrap();

        assert_eq!(result, "Question: What is your favorite color?");
    }

    #[test]
    fn test_description() {
        assert!(AskFollowUpQuestion.description().len() > 100)
    }
}
