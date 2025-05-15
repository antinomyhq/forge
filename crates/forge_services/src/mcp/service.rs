use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{
    McpConfig, McpConfigManager, McpServer as McpServerConfig, McpService, Tool, ToolDefinition,
    ToolName,
};
use tokio::sync::{Mutex, RwLock};

use crate::mcp::tool::McpTool;
use crate::{Infrastructure, McpClient, McpServer};

#[derive(Clone)]
pub struct ForgeMcpService<R, I> {
    tools: Arc<RwLock<HashMap<ToolName, Arc<Tool>>>>,
    previous_config_hash: Arc<Mutex<u64>>,
    reader: Arc<R>,
    infra: Arc<I>,
}

impl<R: McpConfigManager, I: Infrastructure> ForgeMcpService<R, I> {
    pub fn new(reader: Arc<R>, infra: Arc<I>) -> Self {
        Self {
            tools: Default::default(),
            previous_config_hash: Arc::new(Mutex::new(0)),
            reader,
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

    async fn insert_tools(
        &self,
        server_name: &str,
        tools: Vec<ToolDefinition>,
        client: Arc<dyn McpClient>,
    ) -> anyhow::Result<()> {
        let mut lock = self.tools.write().await;

        for tool in tools.into_iter() {
            let server = McpTool::new(server_name.to_string(), tool.clone(), client.clone())?;
            lock.insert(
                tool.name.clone(),
                Arc::new(Tool { definition: tool, executable: Box::new(server) }),
            );
        }

        Ok(())
    }

    async fn connect_stdio_server(
        &self,
        server_name: &str,
        config: McpServerConfig,
    ) -> anyhow::Result<()> {
        let command = config
            .command
            .ok_or_else(|| anyhow::anyhow!("Command is required for stdio server"))?;
        let env = config.env.unwrap_or_default();
        let args = config.args;

        let client = Arc::new(
            self.infra
                .mcp_server()
                .connect_stdio(server_name, &command, env, args)
                .await?,
        );
        let tools = client
            .list()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list tools: {e}"))?;

        self.insert_tools(server_name, tools, client)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to insert tools: {e}"))?;

        Ok(())
    }
    async fn connect_http_server(
        &self,
        server_name: &str,
        config: McpServerConfig,
    ) -> anyhow::Result<()> {
        let url = config
            .url
            .ok_or_else(|| anyhow::anyhow!("URL is required for HTTP server"))?;
        let client = Arc::new(
            self.infra
                .mcp_server()
                .connect_sse(server_name, &url)
                .await?,
        );

        let tools = client.list().await?;
        self.insert_tools(server_name, tools, client.clone())
            .await?;

        Ok(())
    }
    async fn init_mcp(&self) -> anyhow::Result<()> {
        let mcp = self.reader.read().await?;

        // If config is unchanged, skip reinitialization
        if !self.is_config_modified(&mcp).await {
            return Ok(());
        }

        // Update the hash with the new config
        let new_hash = Self::hash(&mcp);
        *self.previous_config_hash.lock().await = new_hash;
        self.clear_tools().await;

        futures::future::join_all(
            mcp.mcp_servers
                .iter()
                .map(|(name, server)| async move {
                    if server.url.is_some() {
                        Some(
                            self.connect_http_server(name, server.clone())
                                .await
                                .context(format!("Failed to initiate MCP server: {name}")),
                        )
                    } else {
                        Some(
                            self.connect_stdio_server(name, server.clone())
                                .await
                                .context(format!("Failed to initiate MCP server: {name}")),
                        )
                    }
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .flatten()
        .filter_map(|e| e.err())
        .next()
        .map_or(Ok(()), Err)
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
