use std::sync::Arc;

use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, ToolCallContext, ToolName};

use crate::McpClient;

pub struct McpExecutor<T> {
    pub client: Arc<T>,
    pub tool_name: ToolName,
}

impl<T> McpExecutor<T> {
    pub fn new(tool_name: ToolName, client: Arc<T>) -> anyhow::Result<Self> {
        Ok(Self { client, tool_name })
    }
}

#[async_trait::async_trait]
impl<T: McpClient> ExecutableTool for McpExecutor<T> {
    type Input = serde_json::Value;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        context
            .send_text(TitleFormat::info("MCP").sub_title(self.tool_name.as_str()))
            .await?;

        self.client.call(&self.tool_name, input).await
    }
}
