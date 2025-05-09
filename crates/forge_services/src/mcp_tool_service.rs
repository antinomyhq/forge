use forge_domain::{
    Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolResult, ToolService,
};

pub struct McpToolService {}

impl Default for McpToolService {
    fn default() -> Self {
        Self::new()
    }
}

impl McpToolService {
    pub fn new() -> McpToolService {
        McpToolService {}
    }
}

#[async_trait::async_trait]
impl ToolService for McpToolService {
    async fn call(&self, _: ToolCallContext, _: ToolCallFull) -> ToolResult {
        todo!()
    }
    fn list(&self) -> Vec<ToolDefinition> {
        todo!()
    }
    fn find(&self, _: &ToolName) -> Option<Tool> {
        todo!()
    }
}
