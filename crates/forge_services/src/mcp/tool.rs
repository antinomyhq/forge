use std::sync::Arc;

use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, ToolCallContext, ToolName};

use crate::McpClient;

pub struct McpTool<T> {
    pub client: Arc<T>,
    pub tool_name: ToolName,
    pub server_name: String,
}

impl<T> McpTool<T> {
    pub fn new(
        server_name: impl ToString,
        tool_name: ToolName,
        client: Arc<T>,
    ) -> anyhow::Result<Self> {
        Ok(Self { client, tool_name, server_name: server_name.to_string() })
    }
}

#[async_trait::async_trait]
impl<T: McpClient> ExecutableTool for McpTool<T> {
    type Input = serde_json::Value;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        context
            .send_text(TitleFormat::debug("MCP").sub_title(self.tool_name.as_str()))
            .await?;
        let mut result = self.client.call_tool(&self.tool_name, input.clone()).await;

        let mut retries = 0;
        while let Err(Some(rmcp::ServiceError::Transport(_))) = result
            .as_ref()
            .map_err(|e| e.downcast_ref::<rmcp::ServiceError>())
        {
            if retries > 2 {
                context
                    .send_text(TitleFormat::error(format!(
                        "Unable to connect to MCP: {}",
                        self.server_name
                    )))
                    .await?;
                break;
            }
            self.client.reconnect().await?;
            result = self.client.call_tool(&self.tool_name, input.clone()).await;
            retries += 1;
        }

        result
    }
}
