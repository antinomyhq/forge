use serde::de::DeserializeOwned;

use crate::{ToolCallFull, ToolDefinition, ToolResult};

#[async_trait::async_trait]
pub trait ExecutableTool {
    type Input: DeserializeOwned;

    async fn call(&self, input: Self::Input) -> Result<String, String>;
}

#[async_trait::async_trait]
pub trait ToolService: Send + Sync {
    async fn call(&self, call: ToolCallFull) -> ToolResult;
    fn list(&self) -> Vec<ToolDefinition>;
    fn usage_prompt(&self) -> String;
}
