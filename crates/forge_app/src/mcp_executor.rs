use std::sync::Arc;

use forge_domain::{TitleFormat, ToolCallContext, ToolCallFull, ToolName, ToolOutput};

use crate::McpService;

pub struct McpExecutor<S> {
    services: Arc<S>,
}

impl<S: McpService> McpExecutor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        context
            .send_tool_input(TitleFormat::info("MCP").sub_title(input.name.as_str()))
            .await?;

        self.services.execute_mcp(input).await
    }

    pub async fn contains_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        let mcp_servers = self.services.get_mcp_servers().await?;
        Ok(mcp_servers
            .get_servers()
            .values()
            .any(|tools| tools.iter().any(|tool| tool.name == *tool_name)))
    }

    /// Returns the server name that owns the given tool, or `None` if not found.
    /// The returned string is the MCP server's key (e.g. "github", "filesystem").
    ///
    /// Note: this method calls `get_mcp_servers()` internally. Callers that have
    /// just called `contains_tool` will trigger two fetches of the server list.
    /// This is acceptable because `get_mcp_servers()` is expected to be cached,
    /// but if that assumption changes, consider combining the two into a single
    /// method that returns `Option<String>` (server name), falsy when not found.
    pub async fn server_for_tool(&self, tool_name: &ToolName) -> anyhow::Result<Option<String>> {
        let mcp_servers = self.services.get_mcp_servers().await?;
        let server_name = mcp_servers
            .get_servers()
            .iter()
            .find(|(_, tools)| tools.iter().any(|tool| tool.name == *tool_name))
            .map(|(name, _)| name.to_string());
        Ok(server_name)
    }
}
