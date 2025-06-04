use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{McpConfig, McpServerConfig, Tool, ToolDefinition, ToolName};
use tokio::sync::{Mutex, RwLock};

use crate::mcp::tool::McpExecutor;
use crate::services::{McpConfigManager, McpService};
use crate::{Infrastructure, McpClient, McpServer};

#[derive(Clone)]
pub struct ForgeMcpService<M, I> {
    tools: Arc<RwLock<HashMap<ToolName, Arc<Tool>>>>,
    previous_config_hash: Arc<Mutex<u64>>,
    manager: Arc<M>,
    infra: Arc<I>,
}

impl<M: McpConfigManager, I: Infrastructure> ForgeMcpService<M, I> {
    pub fn new(manager: Arc<M>, infra: Arc<I>) -> Self {
        Self {
            tools: Default::default(),
            previous_config_hash: Arc::new(Mutex::new(0)),
            manager,
            infra,
        }
    }

    fn hash(config: &McpConfig) -> u64 {
        let mut hasher = DefaultHasher::new();
        config.hash(&mut hasher);
        hasher.finish()
    }
    async fn is_config_modified(&self, config: &McpConfig) -> bool {
        *self.previous_config_hash.lock().await != Self::hash(config)
    }

    async fn insert_clients<T: McpClient>(
        &self,
        server_name: &str,
        client: Arc<T>,
    ) -> anyhow::Result<()> {
        let tools = client.list().await?;

        let mut tool_map = self.tools.write().await;

        for mut tool in tools.into_iter() {
            let server = McpExecutor::new(tool.name.clone(), client.clone())?;
            // Generate a unique name for the tool
            let tool_name = ToolName::new(format!("mcp_{server_name}_tool_{}", tool.name));
            tool.name = tool_name.clone();
            tool_map.insert(
                tool_name,
                Arc::new(Tool { definition: tool, executable: Box::new(server) }),
            );
        }

        Ok(())
    }

    async fn connect(&self, server_name: &str, config: McpServerConfig) -> anyhow::Result<()> {
        let client = Arc::new(self.infra.mcp_server().connect(config).await?);
        self.insert_clients(server_name, client).await?;

        Ok(())
    }

    async fn init_mcp(&self) -> anyhow::Result<()> {
        let mcp = self.manager.read().await?;

        // If config is unchanged, skip reinitialization
        if !self.is_config_modified(&mcp).await {
            return Ok(());
        }

        self.update_mcp(mcp).await
    }

    async fn update_mcp(&self, mcp: McpConfig) -> Result<(), anyhow::Error> {
        // Update the hash with the new config
        let new_hash = Self::hash(&mcp);
        *self.previous_config_hash.lock().await = new_hash;
        self.clear_tools().await;

        futures::future::join_all(mcp.mcp_servers.iter().map(|(name, server)| async move {
            self.connect(name, server.clone())
                .await
                .context(format!("Failed to initiate MCP server: {name}"))
        }))
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()
        .map(|_| ())
    }

    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
        self.init_mcp().await?;

        Ok(self.tools.read().await.get(name).cloned())
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.init_mcp().await?;
        Ok(self
            .tools
            .read()
            .await
            .values()
            .map(|tool| tool.definition.clone())
            .collect())
    }
    async fn clear_tools(&self) {
        self.tools.write().await.clear()
    }
}

#[async_trait::async_trait]
impl<R: McpConfigManager, I: Infrastructure> McpService for ForgeMcpService<R, I> {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.list().await
    }

    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>> {
        self.find(name).await
    }
}
