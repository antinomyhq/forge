use std::sync::Arc;

use anyhow::Context;
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, ToolCallContext, ToolDefinition, ToolName};

use crate::McpClient;

pub struct McpTool {
    pub client: Arc<dyn McpClient>,
    pub local_tool_name: ToolName,
}

impl McpTool {
    pub fn new(
        server: String,
        tool: ToolDefinition,
        client: Arc<dyn McpClient>,
    ) -> anyhow::Result<Self> {
        let local_tool_name = ToolName::new(
            tool.name
                .as_str()
                .strip_prefix(&format!("{server}_tool_"))
                .context("Invalid tool name")?,
        );

        Ok(Self { client, local_tool_name })
    }
}

#[async_trait::async_trait]
impl ExecutableTool for McpTool {
    type Input = serde_json::Value;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        context
            .send_text(TitleFormat::debug("MCP").sub_title(self.local_tool_name.as_str()))
            .await?;
        self.client.call_tool(&self.local_tool_name, input).await
    }
}
