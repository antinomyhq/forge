use forge_tool_macros::Description as DescriptionDerive;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Description, ToolTrait};

#[derive(Deserialize, JsonSchema)]
pub struct AskFollowUpQuestionInput {
    pub question: String,
}

/// Ask the user a question to gather additional information needed to complete
/// the task. This tool should be used when you encounter ambiguities, need
/// clarification, or require more details to proceed effectively. It allows for
/// interactive problem-solving by enabling direct communication with the user.
#[derive(DescriptionDerive)]
pub struct AskFollowUpQuestion;

#[async_trait::async_trait]
impl ToolTrait for AskFollowUpQuestion {
    type Input = AskFollowUpQuestionInput;
    type Output = String;

    async fn call(&self, input: Self::Input) -> Result<Self::Output, String> {
        Ok(format!("Question: {}", input.question))
    }
}

#[cfg(test)]
mod test {
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
        assert!(AskFollowUpQuestion::description().len() > 100)
    }
}
