use std::sync::Arc;

use forge_domain::{TitleFormat, ToolCallContext, ToolCallFull, ToolName, ToolOutput};

use crate::McpService;

pub struct McpExecutor<S> {
    pub services: Arc<S>,
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
            .send_title(TitleFormat::info("MCP").sub_title(input.name.as_str()))
            .await?;

        self.services.call(input).await
    }

    pub async fn contains_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        let mcp_tools = self.services.list().await?;
        Ok(mcp_tools
            .values()
            .any(|tools| tools.iter().any(|tool| tool.name == *tool_name)))
    }
}
