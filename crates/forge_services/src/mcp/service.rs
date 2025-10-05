use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use forge_app::domain::{
    McpConfig, McpServerConfig, McpToolCache, ToolCallFull, ToolDefinition, ToolName, ToolOutput,
};
use forge_app::{McpConfigManager, McpService};
use tokio::sync::{Mutex, RwLock};

use crate::mcp::tool::McpExecutor;
use crate::{CacheRepository, McpClientInfra, McpServerInfra};

#[derive(Clone)]
pub struct ForgeMcpService<M, I, C, R> {
    tools: Arc<RwLock<HashMap<ToolName, ToolHolder<McpExecutor<C>>>>>,
    previous_config_hash: Arc<Mutex<String>>,
    manager: Arc<M>,
    infra: Arc<I>,
    cache_repo: Arc<R>,
}

#[derive(Clone)]
struct ToolHolder<T> {
    definition: ToolDefinition,
    executable: T,
    server_name: String,
}

impl<M: McpConfigManager, I: McpServerInfra, C, R> ForgeMcpService<M, I, C, R>
where
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
    R: CacheRepository,
{
    pub fn new(manager: Arc<M>, infra: Arc<I>, cache_repo: Arc<R>) -> Self {
        Self {
            tools: Default::default(),
            previous_config_hash: Arc::new(Mutex::new(String::new())),
            manager,
            infra,
            cache_repo,
        }
    }

    async fn is_config_modified(&self, config: &McpConfig) -> bool {
        *self.previous_config_hash.lock().await != config.cache_key()
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
                ToolHolder {
                    definition: tool,
                    executable: server,
                    server_name: server_name.to_string(),
                },
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

    /// List tools using unified cache
    ///
    /// Uses a single cache entry keyed by the hash of merged user+local
    /// configs. If cache is valid (<24h old and hash matches), returns
    /// cached tools immediately. Otherwise, fetches from MCP servers and
    /// updates the cache.
    async fn list_cached(&self) -> anyhow::Result<HashMap<String, Vec<ToolDefinition>>> {
        // Read current configs to compute merged hash
        let mcp_config = self.manager.read_mcp_config().await?;

        tracing::debug!("MCP cache check: servers={}", mcp_config.mcp_servers.len(),);

        // Compute unified hash from merged config
        let config_hash = mcp_config.cache_key();

        tracing::debug!("Computed merged config hash: {}", config_hash);

        // Check if cache is valid (exists and not expired)
        if self.cache_repo.cache_is_valid(&config_hash).await? {
            // Cache is valid, retrieve it
            if let Some(cache) = self
                .cache_repo
                .cache_get::<String, McpToolCache>(&config_hash)
                .await?
            {
                let age_seconds = self
                    .cache_repo
                    .cache_get_age(&config_hash)
                    .await?
                    .unwrap_or(0);

                tracing::debug!(
                    "Using cached MCP tools (age: {}s, {} servers, {} total tools)",
                    age_seconds,
                    cache.tools.len(),
                    cache.tools.values().map(|v| v.len()).sum::<usize>()
                );
                return Ok(cache.tools.into_iter().collect());
            }
        }

        tracing::debug!("MCP cache invalid or expired, fetching from servers");

        // Cache miss or invalid - fetch from both configs
        let config = !mcp_config.mcp_servers.is_empty();

        // Fetch from both configs if needed
        let mcp_live = if config {
            self.connect_and_list(&mcp_config).await?
        } else {
            HashMap::new()
        };

        // Prefix tool names before caching to match internal registry format
        let prefix_tool_names =
            |tools: HashMap<String, Vec<ToolDefinition>>| -> HashMap<String, Vec<ToolDefinition>> {
                tools
                    .into_iter()
                    .map(|(server_name, tools)| {
                        let prefixed_tools = tools
                            .into_iter()
                            .map(|mut tool| {
                                let generated_name = ToolName::new(format!(
                                    "mcp_{server_name}_tool_{}",
                                    tool.name.clone().into_sanitized()
                                ));
                                tool.name = generated_name;
                                tool
                            })
                            .collect();
                        (server_name, prefixed_tools)
                    })
                    .collect()
            };

        // Prefix all tools
        let mcp_live = prefix_tool_names(mcp_live);

        // Cache the merged result if we have any tools
        if !mcp_live.is_empty() {
            let cache =
                McpToolCache::new(config_hash.clone(), mcp_live.clone().into_iter().collect());
            if let Err(e) = self.cache_repo.cache_set(&config_hash, &cache).await {
                tracing::warn!("Failed to cache MCP tools: {}", e);
            } else {
                tracing::debug!(
                    "Cached {} MCP servers with {} total tools",
                    mcp_live.len(),
                    mcp_live.values().map(|v| v.len()).sum::<usize>()
                );
            }
        }

        Ok(mcp_live.into_iter().collect())
    }

    /// Connect to MCP servers in config and list their tools
    async fn connect_and_list(
        &self,
        config: &McpConfig,
    ) -> anyhow::Result<HashMap<String, Vec<ToolDefinition>>> {
        let mut tools_by_server = HashMap::new();

        for (server_name, server_config) in config.mcp_servers.iter() {
            match self.infra.connect(server_config.clone()).await {
                Ok(client) => {
                    let client = Arc::new(C::from(client));
                    match client.list().await {
                        Ok(tools) => {
                            tools_by_server.insert(server_name.clone(), tools);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to list tools from MCP server '{}': {}",
                                server_name,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to MCP server '{}': {}", server_name, e);
                }
            }
        }

        Ok(tools_by_server)
    }

    async fn list(&self) -> anyhow::Result<std::collections::HashMap<String, Vec<ToolDefinition>>> {
        self.init_mcp().await?;

        let tools = self.tools.read().await;
        let mut grouped_tools = std::collections::HashMap::new();

        for tool in tools.values() {
            grouped_tools
                .entry(tool.server_name.clone())
                .or_insert_with(Vec::new)
                .push(tool.definition.clone());
        }

        Ok(grouped_tools)
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
        let _tools = self.list().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<M: McpConfigManager, I: McpServerInfra, C, R> McpService for ForgeMcpService<M, I, C, R>
where
    C: McpClientInfra + Clone,
    C: From<<I as McpServerInfra>::Client>,
    R: CacheRepository,
{
    type McpCacheRepository = R;

    fn cache_repository(&self) -> &Self::McpCacheRepository {
        &self.cache_repo
    }

    async fn list(&self) -> anyhow::Result<std::collections::HashMap<String, Vec<ToolDefinition>>> {
        self.list().await
    }

    async fn list_cached(
        &self,
    ) -> anyhow::Result<std::collections::HashMap<String, Vec<ToolDefinition>>> {
        self.list_cached().await
    }

    async fn call(&self, call: ToolCallFull) -> anyhow::Result<ToolOutput> {
        self.call(call).await
    }

    async fn reload_mcp(&self) -> anyhow::Result<()> {
        self.refresh_cache().await
    }
}
