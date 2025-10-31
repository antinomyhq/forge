use std::sync::Arc;

use forge_app::McpClientInfra;
use forge_app::domain::{ToolName, ToolOutput};

#[derive(Clone)]
pub struct McpExecutor<T> {
    pub client: Arc<T>,
    pub tool_name: ToolName,
}

impl<T: McpClientInfra> McpExecutor<T> {
    pub fn new(tool_name: ToolName, client: Arc<T>) -> anyhow::Result<Self> {
        Ok(Self { client, tool_name })
    }
    pub async fn call_tool(&self, input: serde_json::Value) -> anyhow::Result<ToolOutput> {
        self.client.call(&self.tool_name, input).await
    }
}
