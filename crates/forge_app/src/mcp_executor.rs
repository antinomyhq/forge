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

    /// Check whether `tool_name` belongs to any MCP server.
    ///
    /// This is a pure in-memory check that does NOT connect to any server.
    /// Tool names are known either because the server connected during a
    /// previous call, or because they were declared statically in the config.
    pub async fn contains_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        self.services.contains_mcp_tool(tool_name).await
    }
}
