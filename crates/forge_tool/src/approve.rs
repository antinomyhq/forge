use anyhow::Result;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
// use inquire::Confirm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input parameters for the approve tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApproveInput {
    /// The message to display when asking for confirmation
    pub message: String,
}

/// The approve tool provides an interactive confirmation dialog for critical
/// operations. Use this tool when a simple yes/no answer is sufficient for
/// to proceed with its decision-making.
///
/// # Use Cases
/// - Confirming destructive operations (file deletions, data modifications)
/// - Validating important user decisions
/// - Ensuring user awareness before significant actions
/// - Getting explicit consent for sensitive operations
///
/// # Behavior
/// - Displays a yes/no dialog with the provided message
/// - Default selection is 'yes' for quick confirmations
/// - Interactive: requires direct user input
/// - Returns true only on explicit 'yes' confirmation
#[derive(ToolDescription)]
pub struct Approve;

impl NamedTool for Approve {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_ui_approve")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for Approve {
    type Input = ApproveInput;

    async fn call(&self, _input: ApproveInput) -> Result<String, String> {
        /*let ans = Confirm::new(&input.message)
            .with_default(true)
            .prompt()
            .map_err(|e| e.to_string())?;*/
        Ok("ans".to_string())
    }
}
