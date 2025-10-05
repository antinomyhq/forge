use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::{
    McpConfig, McpServerConfig, McpServers, ServerName, ToolCallFull, ToolDefinition, ToolName,
    ToolOutput,
};
use forge_app::{McpConfigManager, McpService};
use tokio::sync::{Mutex, RwLock};

use crate::mcp::tool::McpExecutor;
use crate::{CacheRepository, McpClientInfra, McpServerInfra};

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
    server_name: String,
}

impl<M, I, C> ForgeMcpService<M, I, C>
where
    M: McpConfigManager,
    I: McpServerInfra + CacheRepository,
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
{
    pub fn new(manager: Arc<M>, infra: Arc<I>) -> Self {
        Self {
            tools: Default::default(),
            previous_config_hash: Arc::new(Mutex::new(Default::default())),
            manager,
            infra,
        }
    }

    async fn is_config_modified(&self, config: &McpConfig) -> bool {
        *self.previous_config_hash.lock().await != config.cache_key()
    }

    async fn insert_clients(&self, server_name: &ServerName, client: Arc<C>) -> anyhow::Result<()> {
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
                ToolHolder {
                    definition: tool,
                    executable: server,
                    server_name: server_name.to_string(),
                },
            );
        }

        Ok(())
    }

    async fn connect(
        &self,
        server_name: &ServerName,
        config: McpServerConfig,
    ) -> anyhow::Result<()> {
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
        let new_hash = mcp.cache_key();
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

    async fn list(&self) -> anyhow::Result<McpServers> {
        self.init_mcp().await?;

        let tools = self.tools.read().await;
        let mut grouped_tools = std::collections::HashMap::new();

        for tool in tools.values() {
            grouped_tools
                .entry(ServerName::from(tool.server_name.clone()))
                .or_insert_with(Vec::new)
                .push(tool.definition.clone());
        }

        Ok(grouped_tools.into())
    }
    async fn clear_tools(&self) {
        self.tools.write().await.clear()
    }

    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        // Ensure MCP connections are initialized before calling tools
        self.init_mcp().await?;

        let tools = self.tools.read().await;

        let tool = tools.get(&call.name).context("Tool not found")?;

        tool.executable.call_tool(call.arguments.parse()?).await
    }

    /// Refresh the MCP cache by fetching fresh data
    async fn refresh_cache(&self) -> anyhow::Result<()> {
        // Fetch fresh tools by calling list() which connects to MCPs
        self.infra.cache_clear().await?;
        let _ = self.get_mcp_servers().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<M: McpConfigManager, I: McpServerInfra + CacheRepository, C> McpService
    for ForgeMcpService<M, I, C>
where
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
{
    async fn get_mcp_servers(&self) -> anyhow::Result<McpServers> {
        // Read current configs to compute merged hash
        let mcp_config = self.manager.read_mcp_config().await?;

        // Compute unified hash from merged config
        let config_hash = mcp_config.cache_key();

        // Check if cache is valid (exists and not expired)
        // Cache is valid, retrieve it
        if let Some(cache) = self.infra.cache_get::<_, McpServers>(&config_hash).await? {
            return Ok(cache.clone());
        }

        let servers = self.list().await?;
        self.infra.cache_set(&config_hash, &servers).await?;
        Ok(servers)
    }

    async fn execute_mcp(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.call(call).await
    }

    async fn reload_mcp(&self) -> anyhow::Result<()> {
        self.refresh_cache().await
    }
}
