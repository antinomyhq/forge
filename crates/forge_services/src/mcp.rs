use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Context;

use rmcp::{Service, ServiceExt};
use rmcp::model::{CallToolRequestParam, CallToolResult, ClientInfo, Implementation};
use rmcp::service::{RunningService, ServiceRole};
use serde_json::Value;
use tokio::sync::Mutex;

use forge_domain::{McpConfig, McpHttpServerConfig, McpService, ToolDefinition, ToolName, VERSION};

struct ServerHolder<Role: ServiceRole, Service: Service<Role>> {
    client: Arc<RunningService<Role, Service>>,
    tool_definition: ToolDefinition,
    server_name: String,
}

/// Currently just a placeholder structure, to be implemented
/// when we add actual server functionality.
pub struct ForgeMcpService<Role: ServiceRole, Service: Service<Role>> {
    servers: Arc<Mutex<HashMap<ToolName, ServerHolder<Role, Service>>>>,
}

impl<R: ServiceRole, S: Service<R>> ForgeMcpService<R, S> {
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
impl<R: ServiceRole, S: Service<R>> McpService for ForgeMcpService<R, S> {
    type Role = R;
    type Service = S;

    async fn init_mcp(&self, config: McpConfig) -> anyhow::Result<()> {
        if let Some(servers) = config.http {
            servers
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
                })
                .collect::<anyhow::Result<Vec<_>>>()
                .await?;
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
        let client = Arc::new(client);

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
                            description: tool.description.to_string(),
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
        let mut servers = self.servers.lock().await;
        let tool_names = servers.iter().filter(|(_, s)| s.server_name == server_name).map(|(k, _)| k.clone()).collect::<Vec<_>>();
        if tool_names.is_empty() {
            return anyhow::bail!("No server found with name {server_name}");
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
            server.cancel().await.map_err(|e| anyhow::anyhow!("Failed to stop server {name}: {e}"))?;
        }
        Ok(())
    }

    async fn get_service(&self, tool_name: &str) -> anyhow::Result<Arc<RunningService<Self::Role, Self::Service>>> {
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
            server.client.call_tool(CallToolRequestParam {
                name: tool_name.to_string(),
                arguments: Some(arguments),
            }).await
        } else {
            Err(anyhow::anyhow!("Server not found"))
        }
    }
}