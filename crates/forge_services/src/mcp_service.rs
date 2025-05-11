use std::sync::Arc;

use forge_domain::{
    McpExecutorService, McpManagementService, McpServer, McpServerConfig, Tool, ToolCallContext,
    ToolCallFull, ToolDefinition, ToolName, ToolResult, ToolService,
};

///
/// Responsible to identifying which mcp.json to load and from where
/// Initialize an MCP client internally that other services can depend upon.
/// MCP resolution alg is defined here - https://docs.anthropic.com/en/docs/claude-code/tutorials#understanding-mcp-server-scopes
pub struct ForgeMcpExecutorService<M> {
    manager: Arc<M>,
}

impl<M> ForgeMcpExecutorService<M> {
    pub fn new(manager: Arc<M>) -> Self {
        ForgeMcpExecutorService { manager }
    }
}

#[async_trait::async_trait]
impl<M: McpManagementService> ToolService for ForgeMcpExecutorService<M> {
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

impl<M: McpManagementService> McpExecutorService for ForgeMcpExecutorService<M> {}

pub struct ForgeMcpManagementService;
impl Default for ForgeMcpManagementService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeMcpManagementService {
    pub fn new() -> Self {
        ForgeMcpManagementService
    }
}

#[async_trait::async_trait]
impl McpManagementService for ForgeMcpManagementService {
    async fn read(&self) -> McpServerConfig {
        todo!()
    }
    async fn update(&self, _: McpServer) -> McpServerConfig {
        todo!()
    }
}
