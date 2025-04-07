use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use rmcp::model::{CallToolRequestParam, CallToolResult, ClientInfo, Implementation};
use rmcp::ServiceExt;
use serde_json::Value;
use tokio::sync::Mutex;

use forge_domain::{McpConfig, McpHttpServerConfig, McpService, RunnableService, ToolDefinition, ToolName, VERSION};

struct ServerHolder {
    client: Arc<RunnableService>,
    tool_definition: ToolDefinition,
    server_name: String,
}

/// Currently just a placeholder structure, to be implemented
/// when we add actual server functionality.
#[derive(Clone)]
pub struct ForgeMcpService {
    servers: Arc<Mutex<HashMap<ToolName, ServerHolder>>>,
}

impl ForgeMcpService {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn client_info() -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation {
                name: "Forge".to_string(),
                version: VERSION.to_string(),
            },
        }
    }
}

#[async_trait::async_trait]
impl McpService for ForgeMcpService {
    async fn init_mcp(&self, config: McpConfig) -> anyhow::Result<()> {
        if let Some(servers) = config.http {
            let results: Vec<anyhow::Result<()>> = futures::future::join_all(servers
                .iter()
                .map(|(server_name, server)| {
                    let server_config = server.clone();
                    async move {
                        if self.is_server_running(&server_name).await? {
                            return Ok(());
                        }
                        self.start_http_server(&server_name, server_config).await?;
                        Ok(())
                    }
                }).collect::<Vec<_>>()).await;
            for i in results {
                if let Err(e) = i {
                    tracing::error!("Failed to start server: {e}");
                }
            }
        }

        Ok(())
    }
    async fn list_tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.servers.lock().await
            .iter()
            .map(|(_, server)| Ok(server.tool_definition.clone()))
            .collect()
    }

    async fn is_server_running(&self, server_name: &str) -> anyhow::Result<bool> {
        let servers = self.servers.lock().await;
        Ok(servers.contains_key(&ToolName::new(server_name)))
    }

    async fn start_http_server(&self, server_name: &str, config: McpHttpServerConfig) -> anyhow::Result<()> {
        let transport = rmcp::transport::SseTransport::start(config.url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start server: {e}"))?;

        let client = Self::client_info()
            .serve(transport)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to serve client: {e}"))?;

        let tools = client.list_tools(None).await.map_err(|e| anyhow::anyhow!("Failed to list tools: {e}"))?;
        let client = Arc::new(RunnableService::Http(client));

        let mut lock = self.servers.lock().await;
        for tool in tools.tools.into_iter() {
            let tool_name = ToolName::prefixed(server_name, tool.name);
            lock
                .insert(
                    tool_name.clone(),
                    ServerHolder {
                        client: client.clone(),
                        tool_definition: ToolDefinition {
                            name: tool_name,
                            description: tool.description.unwrap_or_default().to_string(),
                            input_schema: serde_json::from_str(&serde_json::to_string(&tool.input_schema)?)?,
                            output_schema: None,
                        },
                        server_name: server_name.to_string(),
                    },
                );
        }
        drop(lock);

        Ok(())
    }

    async fn stop_server(&self, server_name: &str) -> anyhow::Result<()> {
        let servers = self.servers.lock().await;
        let tool_names = servers.iter().filter(|(_, s)| s.server_name == server_name).map(|(k, _)| k.clone()).collect::<Vec<_>>();
        
        if tool_names.is_empty() {
            return Err(anyhow::anyhow!("No server found with name {}", server_name));
        }
        let mut lock = self.servers.lock().await;
        for tool_name in tool_names {
            if let Some(removed) = lock.remove(&tool_name) {
                Arc::into_inner(removed.client).context(anyhow::anyhow!("Failed to stop server"))?.cancel().await.map_err(|e| anyhow::anyhow!("Failed to stop server: {e}"))?;
            }
        }
        drop(lock);
        Ok(())
    }

    async fn stop_all_servers(&self) -> anyhow::Result<()> {
        let mut servers = self.servers.lock().await;
        for (name, server) in servers.drain() {
            Arc::into_inner(server.client).context(anyhow::anyhow!("Failed to stop server {name}"))?
                .cancel().await.map_err(|e| anyhow::anyhow!("Failed to stop server {name}: {e}"))?;
        }
        Ok(())
    }

    async fn get_service(&self, tool_name: &str) -> anyhow::Result<Arc<RunnableService>> {
        let servers = self.servers.lock().await;
        if let Some(server) = servers.get(&ToolName::new(tool_name)) {
            Ok(server.client.clone())
        } else {
            Err(anyhow::anyhow!("Server not found"))
        }
    }

    async fn call_tool(&self, tool_name: &str, arguments: Value) -> anyhow::Result<CallToolResult> {
        let servers = self.servers.lock().await;
        if let Some(server) = servers.get(&ToolName::new(tool_name)) {
            Ok(server.client.call_tool(CallToolRequestParam {
                name: Cow::Owned(tool_name.to_string()),
                arguments: arguments.as_object().cloned(),
            }).await?)
        } else {
            Err(anyhow::anyhow!("Server not found"))
        }
    }
}