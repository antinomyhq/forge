use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    LoaderService, McpFsServerConfig, McpHttpServerConfig, McpService, RunnableService,
    ToolDefinition, ToolName, VERSION,
};
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientInfo, Implementation, ListToolsResult,
};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::Mutex;

struct ServerHolder {
    client: Arc<RunnableService>,
    tool_definition: ToolDefinition,
}

/// Currently just a placeholder structure, to be implemented
/// when we add actual server functionality.
#[derive(Clone)]
pub struct ForgeMcp {
    servers: Arc<Mutex<HashMap<ToolName, ServerHolder>>>,
    loader: Arc<dyn LoaderService>,
}

impl ForgeMcp {
    pub fn new(loader: Arc<dyn LoaderService>) -> Self {
        Self { servers: Arc::new(Mutex::new(HashMap::new())), loader }
    }
    pub fn client_info() -> ClientInfo {
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
        let mut lock = self.servers.lock().await;
        for tool in tools.tools.into_iter() {
            let tool_name = ToolName::prefixed(server_name, tool.name);
            lock.insert(
                tool_name.clone(),
                ServerHolder {
                    client: client.clone(),
                    tool_definition: ToolDefinition {
                        name: tool_name,
                        description: tool.description.unwrap_or_default().to_string(),
                        input_schema: serde_json::from_str(&serde_json::to_string(
                            &tool.input_schema,
                        )?)?,
                        output_schema: None,
                    },
                },
            );
        }

        Ok(())
    }

    async fn start_fs_server(
        &self,
        server_name: &str,
        config: McpFsServerConfig,
    ) -> anyhow::Result<()> {
        let mut command = Command::new(config.command);

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
    async fn start_http_server(
        &self,
        server_name: &str,
        config: McpHttpServerConfig,
    ) -> anyhow::Result<()> {
        let transport = rmcp::transport::SseTransport::start(config.url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start server: {e}"))?;

        let client = Self::client_info()
            .serve(transport)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to serve client: {e}"))?;

        let tools = client
            .list_tools(None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list tools: {e}"))?;
        let client = Arc::new(RunnableService::Http(client));

        self.insert_tools(server_name, tools, client.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to insert tools: {e}"))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl McpService for ForgeMcp {
    async fn init_mcp(&self) -> anyhow::Result<()> {
        let _ = self.stop_all_servers().await;
        match self.loader.load().await?.mcp {
            None => Ok(()),
            Some(config) => {
                if let Some(http_servers) = config.http {
                    let http_results: Vec<anyhow::Result<()>> = futures::future::join_all(
                        http_servers
                            .iter()
                            .map(|(server_name, server)| {
                                self.start_http_server(server_name, server.clone())
                            })
                            .collect::<Vec<_>>(),
                    )
                    .await;

                    for i in http_results {
                        if let Err(e) = i {
                            tracing::error!("Failed to start server: {e}");
                        }
                    }
                }

                if let Some(fs_servers) = config.fs {
                    let fs_results: Vec<anyhow::Result<()>> = futures::future::join_all(
                        fs_servers
                            .iter()
                            .map(|(server_name, server)| {
                                self.start_fs_server(server_name, server.clone())
                            })
                            .collect::<Vec<_>>(),
                    )
                    .await;

                    for i in fs_results {
                        if let Err(e) = i {
                            tracing::error!("Failed to start server: {e}");
                        }
                    }
                }

                Ok(())
            }
        }
    }
    async fn list_tools(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.servers
            .lock()
            .await
            .iter()
            .map(|(_, server)| Ok(server.tool_definition.clone()))
            .collect()
    }

    async fn stop_all_servers(&self) -> anyhow::Result<()> {
        let mut servers = self.servers.lock().await;
        for (name, server_holder) in servers.drain() {
            // Get the Arc from the server holder
            let client = server_holder.client;

            // Log reference count info if high
            let ref_count = Arc::strong_count(&client);
            if ref_count > 1 {
                continue;
            }

            // Try to get exclusive ownership and call cancel
            if let Ok(service) = Arc::try_unwrap(client) {
                service
                    .cancel()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to stop server {name}: {e}"))?;
            }
        }
        servers.clear();
        Ok(())
    }

    async fn call_tool(&self, tool_name: &str, arguments: Value) -> anyhow::Result<CallToolResult> {
        let tool_name = ToolName::new(tool_name);
        let servers = self.servers.lock().await;
        if let Some(server) = servers.get(&tool_name) {
            Ok(server
                .client
                .call_tool(CallToolRequestParam {
                    name: Cow::Owned(tool_name.strip_prefix()),
                    arguments: arguments.as_object().cloned(),
                })
                .await?)
        } else {
            Err(anyhow::anyhow!("Server not found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use forge_domain::{LoaderService, McpConfig, McpHttpServerConfig, McpService, Workflow};
    use rmcp::model::{CallToolResult, Content};
    use rmcp::transport::SseServer;
    use rmcp::{tool, ServerHandler};
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    use crate::mcp::ForgeMcp;

    struct MockLoaderService {
        workflow: Workflow,
    }

    impl MockLoaderService {
        fn from_http<I: IntoIterator<Item = (String, McpHttpServerConfig)>>(http: I) -> Self {
            Self {
                workflow: Workflow::default()
                    .mcp(McpConfig::default().http(http.into_iter().collect())),
            }
        }
    }

    #[async_trait::async_trait]
    impl LoaderService for MockLoaderService {
        async fn load(&self) -> anyhow::Result<Workflow> {
            Ok(self.workflow.clone())
        }
    }

    const MOCK_URL: &str = "127.0.0.1:19194";

    #[derive(Clone)]
    pub struct Counter {
        counter: Arc<Mutex<i32>>,
    }

    #[tool(tool_box)]
    impl Counter {
        pub fn new() -> Self {
            Self { counter: Arc::new(Mutex::new(0)) }
        }

        #[tool(description = "Increment the counter by 1")]
        async fn increment(&self) -> anyhow::Result<CallToolResult, rmcp::Error> {
            let mut counter = self.counter.lock().await;
            *counter += 1;
            Ok(CallToolResult::success(vec![Content::text(
                counter.to_string(),
            )]))
        }
    }

    #[tool(tool_box)]
    impl ServerHandler for Counter {}

    async fn start_server() -> anyhow::Result<CancellationToken> {
        let ct = SseServer::serve(MOCK_URL.parse()?)
            .await?
            .with_service(Counter::new);
        Ok(ct)
    }

    #[tokio::test]
    async fn test_increment() {
        let ct = start_server().await.unwrap();

        let mut map = HashMap::new();
        map.insert(
            "test".to_string(),
            McpHttpServerConfig { url: format!("http://{MOCK_URL}/sse") },
        );
        let mcp = ForgeMcp::new(Arc::new(MockLoaderService::from_http(map)));
        mcp.init_mcp().await.unwrap();
        let tools = mcp.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.strip_prefix(), "increment");

        let one = mcp
            .call_tool("test-forgestrip-increment", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(one.content[0].as_text().unwrap().text, "1");
        let two = mcp
            .call_tool("test-forgestrip-increment", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(two.content[0].as_text().unwrap().text, "2");
        ct.cancel();
    }
}
