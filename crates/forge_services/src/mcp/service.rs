use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{McpConfigManager, McpServer, McpService, Tool, ToolDefinition, ToolName};
use futures::FutureExt;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientInfo, Implementation, InitializeRequestParam,
    ListToolsResult,
};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceError, ServiceExt};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::mcp::executor::McpExecutor;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

pub enum RunnableService {
    Http(RunningService<RoleClient, InitializeRequestParam>),
    Fs(RunningService<RoleClient, ()>),
}

impl RunnableService {
    pub async fn call_tool(
        &self,
        params: CallToolRequestParam,
    ) -> Result<CallToolResult, ServiceError> {
        match self {
            RunnableService::Http(service) => service.call_tool(params).await,
            RunnableService::Fs(service) => service.call_tool(params).await,
        }
    }
}

#[derive(Clone)]
pub struct ForgeMcpService<R> {
    tools: Arc<Mutex<HashMap<ToolName, Arc<Tool>>>>,
    reader: Arc<R>,
}

impl<R: McpConfigManager> ForgeMcpService<R> {
    pub fn new(reader: Arc<R>) -> Self {
        Self { tools: Arc::new(Mutex::new(HashMap::new())), reader }
    }

    fn client_info() -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation { name: "Forge".to_string(), version: VERSION.to_string() },
        }
    }

    async fn insert_tools(
        &self,
        server_name: &str,
        tools: ListToolsResult,
        client: Arc<RunnableService>,
    ) -> anyhow::Result<()> {
        let mut lock = self.tools.lock().await;
        for tool in tools.tools.into_iter() {
            let server = McpExecutor::new(server_name.to_string(), tool.clone(), client.clone())?;
            lock.insert(
                server.tool_definition.name.clone(),
                Arc::new(Tool {
                    definition: server.tool_definition.clone(),
                    executable: Box::new(server),
                }),
            );
        }

        Ok(())
    }

    async fn connect_stdio_server(
        &self,
        server_name: &str,
        config: McpServer,
    ) -> anyhow::Result<()> {
        let command = config
            .command
            .ok_or_else(|| anyhow::anyhow!("Command is required for FS server"))?;

        let mut command = Command::new(command);

        if let Some(env) = config.env {
            for (key, value) in env {
                command.env(key, value);
            }
        }

        let client = ().serve(TokioChildProcess::new(command.args(config.args))?).await?;
        let tools = client
            .list_tools(None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list tools: {e}"))?;
        let client = Arc::new(RunnableService::Fs(client));

        self.insert_tools(server_name, tools, client.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to insert tools: {e}"))?;

        Ok(())
    }
    async fn connect_http_server(
        &self,
        server_name: &str,
        config: McpServer,
    ) -> anyhow::Result<()> {
        let url = config
            .url
            .ok_or_else(|| anyhow::anyhow!("URL is required for HTTP server"))?;
        let transport = rmcp::transport::SseTransport::start(url).await?;
        let client = Self::client_info().serve(transport).await?;
        let tools = client.list_tools(None).await?;
        let client = Arc::new(RunnableService::Http(client));
        self.insert_tools(server_name, tools, client.clone())
            .await?;

        Ok(())
    }
    async fn init_mcp(&self) -> anyhow::Result<()> {
        let mcp = self.reader.read().await?.mcp_servers;
        futures::future::join_all(
            mcp.iter()
                .map(|(name, server)| async move {
                    if self
                        .tools
                        .lock()
                        .map(|v| v.values().any(|v| v.definition.name.to_string().eq(name)))
                        .await
                    {
                        None
                    } else if server.url.is_some() {
                        Some(self.connect_http_server(name, server.clone()).await)
                    } else {
                        Some(self.connect_stdio_server(name, server.clone()).await)
                    }
                })
                // TODO: use flatten function provided by FuturesExt
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .flatten()
        .filter_map(|e| e.err())
        .next()
        .map_or(Ok(()), Err)
    }

    async fn find(&self, name: &ToolName) -> Option<Arc<Tool>> {
        self.tools.lock().await.get(name).cloned()
    }
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.init_mcp().await?;
        Ok(self
            .tools
            .lock()
            .await
            .values()
            .map(|tool| tool.definition.clone())
            .collect())
    }
}

#[async_trait::async_trait]
impl<R: McpConfigManager> McpService for ForgeMcpService<R> {
    async fn list(&self) -> Vec<ToolDefinition> {
        self.list().await.unwrap_or_default()
    }

    async fn find(&self, name: &ToolName) -> Option<Arc<Tool>> {
        self.find(name).await
    }
}
