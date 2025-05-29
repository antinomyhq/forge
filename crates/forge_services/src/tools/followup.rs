use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    ExecutableTool, NamedTool, SelectInput, ToolCallContext, ToolDescription, ToolOutput,
};
use forge_tool_macros::ToolDescription;

use crate::infra::InquireService;
use crate::Infrastructure;

/// Use this tool when you encounter ambiguities, need clarification, or require
/// more details to proceed effectively. Use this tool judiciously to maintain a
/// balance between gathering necessary information and avoiding excessive
/// back-and-forth.
#[derive(Debug, ToolDescription)]
pub struct Followup<F> {
    infra: Arc<F>,
}

impl<F> Followup<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: Infrastructure> NamedTool for Followup<F> {
    fn tool_name() -> forge_domain::ToolName {
        forge_domain::ToolName::new("forge_tool_followup")
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for Followup<F> {
    type Input = SelectInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> Result<ToolOutput> {
        let options = vec![
            input.option1,
            input.option2,
            input.option3,
            input.option4,
            input.option5,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let inquire = self.infra.inquire_service();

        let result = match (options.is_empty(), input.multiple.unwrap_or_default()) {
            (true, _) => inquire.prompt_question(&input.question).await?,
            (false, true) => inquire
                .select_many(&input.question, options)
                .await?
                .map(|selected| {
                    format!(
                        "User selected {} option(s): {}",
                        selected.len(),
                        selected.join(", ")
                    )
                }),
            (false, false) => inquire
                .select_one(&input.question, options)
                .await?
                .map(|selected| format!("User selected: {selected}")),
        };

        match result {
            Some(answer) => Ok(ToolOutput::text(answer)),
            None => {
                context.set_complete().await;
                Ok(ToolOutput::text(
                    "User interrupted the selection".to_string(),
                ))
            }
        }
    }
}
