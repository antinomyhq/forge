/// Errors that can occur in the infrastructure layer
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported MCP response: {0}")]
    UnsupportedMcpResponse(&'static str),

    #[error("MCP server '{server}' error: {message}")]
    McpServerError { server: String, message: String },

    #[error("MCP connection failed for server '{server}': {reason}")]
    McpConnectionFailed { server: String, reason: String },

    #[error("MCP tool '{tool}' execution failed on server '{server}': {reason}")]
    McpToolExecutionFailed {
        server: String,
        tool: String,
        reason: String,
    },
}
