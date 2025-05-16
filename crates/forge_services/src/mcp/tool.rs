use std::sync::Arc;

use anyhow::Context as _;
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, ToolCallContext, ToolName};

use crate::McpClient;

pub struct McpExecutor<T> {
    pub client: Arc<T>,
    pub tool_name: ToolName,
    pub server_name: String,
}

impl<T> McpExecutor<T> {
    pub fn new(
        server_name: impl ToString,
        tool_name: ToolName,
        client: Arc<T>,
    ) -> anyhow::Result<Self> {
        Ok(Self { client, tool_name, server_name: server_name.to_string() })
    }
}

#[async_trait::async_trait]
impl<T: McpClient> ExecutableTool for McpExecutor<T> {
    type Input = serde_json::Value;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        context
            .send_text(TitleFormat::info("MCP").sub_title(self.tool_name.as_str()))
            .await?;

        self.client
            .call(&self.tool_name, input)
            .await
            .context(format!(
                "Failed to connect to MCP server: {}",
                self.server_name
            ))
    }
}
