use forge_domain::{AgentId, ToolName};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid tool call arguments: {0}")]
    ToolCallArgument(serde_json::Error),

    #[error("Tool {0} not found")]
    ToolNotFound(ToolName),

    #[error("Tool '{tool_name}' timed out after {timeout} minutes")]
    ToolCallTimeout { tool_name: ToolName, timeout: u64 },

    #[error(
        "No tool with name '{name}' is supported by agent '{agent}'. Please try again with one of these tools {supported_tools}"
    )]
    ToolNotAllowed {
        name: ToolName,
        agent: AgentId,
        supported_tools: String,
    },
}
