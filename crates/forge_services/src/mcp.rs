use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{
    ConversationId, ConversationService, McpServerConfig, McpServers, Tool, ToolCallContext,
    ToolCallFull, ToolDefinition, ToolName, ToolResult, ToolService,
};
use futures::FutureExt;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientInfo, Implementation, InitializeRequestParam,
    ListToolsResult,
};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceError, ServiceExt};
use schemars::schema::RootSchema;
use tokio::process::Command;
use tokio::sync::Mutex;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

enum RunnableService {
    Http(RunningService<RoleClient, InitializeRequestParam>),
    Fs(RunningService<RoleClient, ()>),
}

impl RunnableService {
    async fn call_tool(
        &self,
        params: CallToolRequestParam,
    ) -> Result<CallToolResult, ServiceError> {
        match self {
            RunnableService::Http(service) => service.call_tool(params).await,
            RunnableService::Fs(service) => service.call_tool(params).await,
        }
    }
}

/// Currently just a placeholder structure, to be implemented
/// when we add actual server functionality.
#[derive(Clone)]
pub struct ForgeMcpService<C> {
    servers: Arc<Mutex<HashMap<ToolName, Server>>>,
    conversation_service: Arc<C>,
}

impl<C: ConversationService> ForgeMcpService<C> {
    pub fn new(conversation_service: Arc<C>) -> Self {
        Self {
            servers: Arc::new(Mutex::new(HashMap::new())),
            conversation_service,
        }
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
            let server = Server::new(server_name.to_string(), tool.clone(), client.clone())?;
            lock.insert(server.tool_definition.name.clone(), server);
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
        config: McpServerConfig,
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
    async fn init_mcp(&self, mcp: McpServers) -> anyhow::Result<()> {
        let http_results: Vec<Option<anyhow::Result<()>>> = futures::future::join_all(
            mcp.iter()
                .map(|(server_name, server)| async move {
                    if self
                        .servers
                        .lock()
                        .map(|v| v.values().any(|v| v.server_name.eq(server_name)))
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

        http_results
            .into_iter()
            .flatten()
            .filter_map(|e| e.err())
            .next()
            .map_or(Ok(()), Err)
    }

    async fn call(&self, ctx: ToolCallContext, call: ToolCallFull) -> anyhow::Result<ToolResult> {
        if ctx.mcp_servers.is_empty() {
            return Err(anyhow::anyhow!("MCP config not defined in the workspace."));
        }
        self.init_mcp(ctx.mcp_servers).await?;

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
            Err(anyhow::anyhow!(
                "MCP server {} not found",
                tool_name.as_str()
            ))
        }
    }
    async fn list(&self, conversation_id: &ConversationId) -> anyhow::Result<Vec<ToolDefinition>> {
        let mcp = self
            .conversation_service
            .find(conversation_id)
            .await?
            .context("Failed to find conversation")?
            .mcp_servers;

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

    fn usage_prompt(&self) -> String {
        todo!()
    }

    fn find_tool(&self, _name: &ToolName) -> Option<&Tool> {
        todo!()
    }
}

#[async_trait::async_trait]
impl<C: ConversationService> ToolService for ForgeMcpService<C> {
    async fn call(&self, ctx: ToolCallContext, call: ToolCallFull) -> anyhow::Result<ToolResult> {
        self.call(ctx, call).await
    }
    async fn list(&self, conversation_id: &ConversationId) -> anyhow::Result<Vec<ToolDefinition>> {
        self.list(conversation_id).await
    }

    fn usage_prompt(&self) -> String {
        self.usage_prompt()
    }

    fn find_tool(&self, name: &ToolName) -> Option<&Tool> {
        self.find_tool(name)
    }
}

struct Server {
    server_name: String,
    client: Arc<RunnableService>,
    tool_definition: ToolDefinition,
}

impl Server {
    pub fn new(
        server_name: String,
        tool: rmcp::model::Tool,
        client: Arc<RunnableService>,
    ) -> anyhow::Result<Self> {
        let name = ToolName::prefixed(server_name.clone(), tool.name);
        let input_schema: RootSchema = serde_json::from_value(serde_json::Value::Object(
            tool.input_schema.as_ref().clone(),
        ))?;

        Ok(Self {
            client,
            tool_definition: ToolDefinition::new(name)
                .description(tool.description.unwrap_or_default().to_string())
                .input_schema(input_schema),
            server_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use forge_domain::{
        CompactionResult, Conversation, ConversationId, ConversationService, McpServerConfig,
        ToolCallContext, ToolCallFull, ToolName, Workflow,
    };
    use rmcp::model::{CallToolResult, Content};
    use rmcp::transport::SseServer;
    use rmcp::{tool, ServerHandler};
    use serde_json::Value;
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    use crate::mcp::ForgeMcpService;

    struct MockCommunicationService {
        workflow: Workflow,
    }

    impl MockCommunicationService {
        fn new(workflow: Workflow) -> Self {
            Self { workflow }
        }
    }

    #[async_trait::async_trait]
    impl ConversationService for MockCommunicationService {
        async fn find(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>> {
            Ok(Some(Conversation {
                id: id.clone(),
                archived: false,
                state: Default::default(),
                variables: Default::default(),
                agents: vec![],
                events: vec![],
                mcp_servers: self.workflow.mcp_servers.clone().unwrap_or_default(),
            }))
        }

        async fn upsert(&self, _: Conversation) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn create(&self, _: Workflow) -> anyhow::Result<Conversation> {
            unimplemented!()
        }

        async fn get_variable(&self, _: &ConversationId, _: &str) -> anyhow::Result<Option<Value>> {
            unimplemented!()
        }

        async fn set_variable(
            &self,
            _: &ConversationId,
            _: String,
            _: Value,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }

        async fn delete_variable(&self, _: &ConversationId, _: &str) -> anyhow::Result<bool> {
            unimplemented!()
        }

        async fn update<F, T>(&self, _: &ConversationId, _: F) -> anyhow::Result<T>
        where
            F: FnOnce(&mut Conversation) -> T + Send,
        {
            unimplemented!()
        }

        async fn compact_conversation(
            &self,
            _: &ConversationId,
        ) -> anyhow::Result<CompactionResult> {
            unimplemented!()
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

        let mut mcp = HashMap::new();
        mcp.insert(
            "test".to_string(),
            McpServerConfig::default().url(format!("http://{MOCK_URL}/sse")),
        );
        let workflow = Workflow::default().mcp_servers(mcp);

        let convo = MockCommunicationService::new(workflow.clone());

        let mcp = ForgeMcpService::new(Arc::new(convo));
        let tools = mcp.list(&ConversationId::generate()).await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.strip_prefix(), "increment");

        let one = mcp
            .call(
                ToolCallContext::default().mcp_servers(workflow.mcp_servers.clone().unwrap()),
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
                ToolCallContext::default().mcp_servers(workflow.mcp_servers.unwrap()),
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

    #[tokio::test]
    async fn test_get_tool() {
        let mut mcp = HashMap::new();
        mcp.insert(
            "test".to_string(),
            McpServerConfig::default().url("http://example.com"),
        );
        let workflow = Workflow::default().mcp_servers(mcp);
        let convo = MockCommunicationService::new(workflow);
        let mcp_service = ForgeMcpService::new(Arc::new(convo));

        // MCP service doesn't store tools locally, so get_tool should always return
        // None
        let tool = mcp_service.find_tool(&ToolName::new("any-tool"));
        assert!(tool.is_none());
    }
}
