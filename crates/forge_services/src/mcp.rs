use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    McpConfig, RunnableService, ToolCallContext, ToolCallFull, ToolDefinition, ToolName,
    ToolResult, ToolService, VERSION,
};
use futures::FutureExt;
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, ListToolsResult};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use tokio::process::Command;
use tokio::sync::Mutex;

struct ServerHolder {
    name: String,
    client: Arc<RunnableService>,
    tool_definition: ToolDefinition,
}

/// Currently just a placeholder structure, to be implemented
/// when we add actual server functionality.
#[derive(Clone)]
pub struct ForgeMcpService {
    servers: Arc<Mutex<HashMap<ToolName, ServerHolder>>>,
}

impl Default for ForgeMcpService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeMcpService {
    pub fn new() -> Self {
        Self { servers: Arc::new(Mutex::new(HashMap::new())) }
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
                    name: server_name.to_string(),
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

    async fn connect_stdio_server(
        &self,
        server_name: &str,
        config: McpConfig,
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
        config: McpConfig,
    ) -> anyhow::Result<()> {
        let url = config
            .url
            .ok_or_else(|| anyhow::anyhow!("URL is required for HTTP server"))?;
        let transport = rmcp::transport::SseTransport::start(url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect server: {e}"))?;

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
    async fn init_mcp(&self, mcp: HashMap<String, McpConfig>) -> anyhow::Result<()> {
        let http_results: Vec<Option<anyhow::Result<()>>> = futures::future::join_all(
            mcp.iter()
                .map(|(server_name, server)| async move {
                    if self
                        .servers
                        .lock()
                        .map(|v| v.values().any(|v| v.name.eq(server_name)))
                        .await
                    {
                        None
                    } else if server.url.is_some() {
                        Some(self.connect_http_server(server_name, server.clone()).await)
                    } else {
                        Some(self.connect_stdio_server(server_name, server.clone()).await)
                    }
                })
                // TODO: use flatten function provided by FuturesExt
                .collect::<Vec<_>>(),
        )
        .await;

        for i in http_results.into_iter().flatten() {
            if let Err(e) = i {
                tracing::error!("Failed to connect server: {e}");
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl ToolService for ForgeMcpService {
    async fn list(&self, mcp: HashMap<String, McpConfig>) -> anyhow::Result<Vec<ToolDefinition>> {
        if !mcp.is_empty() {
            self.init_mcp(mcp)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to init mcp: {e}"))?;
            self.servers
                .lock()
                .await
                .iter()
                .map(|(_, server)| Ok(server.tool_definition.clone()))
                .collect()
        } else {
            Ok(vec![])
        }
    }
    async fn call(&self, ctx: ToolCallContext, call: ToolCallFull) -> anyhow::Result<ToolResult> {
        if ctx.mcp.is_empty() {
            return Err(anyhow::anyhow!("MCP config not defined in the workspace."));
        }
        self.init_mcp(ctx.mcp)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to init mcp: {e}"))?;

        let tool_name = ToolName::new(call.name);
        let servers = self.servers.lock().await;
        if let Some(server) = servers.get(&tool_name) {
            let result = server
                .client
                .call_tool(CallToolRequestParam {
                    name: Cow::Owned(tool_name.strip_prefix()),
                    arguments: call.arguments.as_object().cloned(),
                })
                .await?;

            Ok(ToolResult {
                name: tool_name,
                call_id: call.call_id.clone(),
                content: serde_json::to_string(&result.content)?,
                is_error: result.is_error.unwrap_or_default(),
            })
        } else {
            Err(anyhow::anyhow!("Server not found"))
        }
    }

    fn usage_prompt(&self) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use forge_domain::{McpConfig, ToolCallContext, ToolCallFull, ToolName, ToolService, Workflow};
    use rmcp::model::{CallToolResult, Content};
    use rmcp::transport::SseServer;
    use rmcp::{tool, ServerHandler};
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    use crate::mcp::ForgeMcpService;

    struct MockLoaderService {
        workflow: Workflow,
    }

    impl MockLoaderService {
        fn from_http<I: IntoIterator<Item = (String, McpConfig)>>(configs: I) -> Self {
            Self {
                workflow: Workflow::default().mcp(configs.into_iter().collect::<HashMap<_, _>>()),
            }
        }
    }

    impl MockLoaderService {
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
            McpConfig::default().url(format!("http://{MOCK_URL}/sse")),
        );
        let loader = MockLoaderService::from_http(map);
        let workflow = loader.load().await.unwrap();

        let mcp = ForgeMcpService::new();
        let tools = mcp.list(workflow.mcp.clone().unwrap()).await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.strip_prefix(), "increment");

        let one = mcp
            .call(
                ToolCallContext::default().mcp(workflow.mcp.clone().unwrap()),
                ToolCallFull {
                    name: ToolName::new("test-forgestrip-increment"),
                    call_id: None,
                    arguments: serde_json::json!({}),
                },
            )
            .await
            .unwrap();
        let content = serde_json::from_str::<Vec<Content>>(&one.content).unwrap();
        assert_eq!(content[0].as_text().unwrap().text, "1");
        let two = mcp
            .call(
                ToolCallContext::default()
                    .mcp(workflow.mcp.unwrap()),
                ToolCallFull {
                    name: ToolName::new("test-forgestrip-increment"),
                    call_id: None,
                    arguments: serde_json::json!({}),
                },
            )
            .await
            .unwrap();
        let content = serde_json::from_str::<Vec<Content>>(&two.content).unwrap();

        assert_eq!(content[0].as_text().unwrap().text, "2");
        ct.cancel();
    }
}
