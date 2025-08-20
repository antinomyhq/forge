use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::{
    McpConfig, McpServerConfig, ToolCallFull, ToolDefinition, ToolName, ToolOutput,
};
use forge_app::{McpConfigManager, McpService};
use tokio::sync::{Mutex, RwLock};

use crate::mcp::tool::McpExecutor;
use crate::{McpClientInfra, McpServerInfra};

#[derive(Clone)]
pub struct ForgeMcpService<M, I, C> {
    tools: Arc<RwLock<HashMap<ToolName, ToolHolder<McpExecutor<C>>>>>,
    previous_config_hash: Arc<Mutex<u64>>,
    manager: Arc<M>,
    infra: Arc<I>,
}

#[derive(Clone)]
struct ToolHolder<T> {
    definition: ToolDefinition,
    executable: T,
}

impl<M: McpConfigManager, I: McpServerInfra, C> ForgeMcpService<M, I, C>
where
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
{
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

    async fn insert_clients(&self, server_name: &str, client: Arc<C>) -> anyhow::Result<()> {
        let tools = client.list().await?;

        let mut tool_map = self.tools.write().await;

        for mut tool in tools.into_iter() {
            let actual_name = tool.name.clone();
            let server = McpExecutor::new(actual_name, client.clone())?;

            // Generate a unique name for the tool
            let generated_name = ToolName::new(format!(
                "mcp_{server_name}_tool_{}",
                tool.name.into_sanitized()
            ));

            tool.name = generated_name.clone();

            tool_map.insert(
                generated_name,
                ToolHolder { definition: tool, executable: server },
            );
        }

        Ok(())
    }

    async fn connect(&self, server_name: &str, config: McpServerConfig) -> anyhow::Result<()> {
        let client = self.infra.connect(config).await?;
        let client = Arc::new(C::from(client));
        self.insert_clients(server_name, client).await?;

        Ok(())
    }

    async fn init_mcp(&self) -> anyhow::Result<()> {
        let mcp = self.manager.read_mcp_config().await?;

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

    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        let tools = self.tools.read().await;

        let tool = tools.get(&call.name).context("Tool not found")?;

        tool.executable.call_tool(call.arguments.parse()?).await
    }
}

#[async_trait::async_trait]
impl<R: McpConfigManager, I: McpServerInfra, C> McpService for ForgeMcpService<R, I, C>
where
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
{
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.list().await
    }

    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.call(call).await
    }
}
