use forge_domain::ToolName;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid tool call arguments: {0}")]
    ToolCallArgument(serde_json::Error),

    #[error("Tool {0} not found")]
    ToolNotFound(ToolName),

    #[error("Tool '{tool_name}' timed out after {timeout} minutes")]
    ToolCallTimeout { tool_name: ToolName, timeout: u64 },

    #[error(
        "Tool '{name}' is not available. Please try again with one of these tools: [{supported_tools}]"
    )]
    ToolNotAllowed {
        name: ToolName,
        supported_tools: String,
    },
}
