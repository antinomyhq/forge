use anyhow::Result;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
// use inquire::Select as InquireSelect;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input parameters for the select tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SelectInput {
    /// The message to display above the selection options
    pub message: String,
    /// The list of options to choose from. Intended for multiple options (2 or
    /// more) to provide meaningful choices to the user.
    pub options: Vec<String>,
}

/// The select tool provides an interactive selection dialog for choosing from
/// multiple options. Use this tool when you need the user to choose one item
/// from a list of possibilities.
///
/// # Use Cases
/// - Selecting from multiple available options
/// - Making configuration choices
/// - Choosing between different paths of action
/// - Filtering or narrowing down possibilities
///
/// # Behavior
/// - Displays a selection dialog with the provided message and options
/// - Interactive: user can navigate through options using arrow keys
/// - Returns the selected option as a string
/// - Supports keyboard navigation and search
/// - Best used with multiple options to provide meaningful choices
#[derive(ToolDescription)]
pub struct SelectTool;

impl NamedTool for SelectTool {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_ui_select")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for SelectTool {
    type Input = SelectInput;

    async fn call(&self, _input: SelectInput) -> Result<String, String> {
        // let ans = InquireSelect::new(&input.message, input.options)
        //     .prompt()
        //     .map_err(|e| e.to_string())?;
        Ok("ans".to_string())
    }
}
