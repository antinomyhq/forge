use std::borrow::Cow;
use std::future::Future;
use std::sync::{Arc, RwLock};

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{Image, McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use forge_services::McpClientInfra;
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::schemars::schema::RootSchema;
use rmcp::service::RunningService;
use rmcp::transport::{SseClientTransport, StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;
use tokio::process::Command;

use crate::error::Error;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

type RmcpClient = RunningService<RoleClient, InitializeRequestParam>;

#[derive(Clone)]
pub struct ForgeMcpClient {
    client: Arc<RwLock<Option<Arc<RmcpClient>>>>,
    config: McpServerConfig,
}

impl ForgeMcpClient {
    pub fn new(config: McpServerConfig) -> Self {
        Self { client: Default::default(), config }
    }

    fn client_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation { name: "Forge".to_string(), version: VERSION.to_string() },
        }
    }

    /// Connects to the MCP server. If `force` is true, it will reconnect even
    /// if already connected.
    async fn connect(&self) -> anyhow::Result<Arc<RmcpClient>> {
        if let Some(client) = self.get_client() {
            Ok(client.clone())
        } else {
            let client = self.create_connection().await?;
            self.set_client(client.clone());
            Ok(client.clone())
        }
    }

    fn get_client(&self) -> Option<Arc<RmcpClient>> {
        let guard = self.client.read().unwrap();
        guard.clone()
    }

    fn set_client(&self, client: Arc<RmcpClient>) {
        let mut guard = self.client.write().unwrap();
        *guard = Some(client);
    }

    async fn create_connection(&self) -> anyhow::Result<Arc<RmcpClient>> {
        let client = match &self.config {
            McpServerConfig::Stdio(stdio) => {
                let mut cmd = Command::new(stdio.command.clone());
                cmd.args(&stdio.args);

                for (key, value) in &stdio.env {
                    cmd.env(key, value);
                }

                cmd.stdin(std::process::Stdio::inherit())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                self.client_info()
                    .serve(TokioChildProcess::new(cmd)?)
                    .await?
            }
            McpServerConfig::Sse(sse) => {
                let transport = SseClientTransport::start(sse.url.clone()).await?;
                self.client_info().serve(transport).await?
            }
            McpServerConfig::StreamableHttp(streamable) => {
                let transport = StreamableHttpClientTransport::from_uri(streamable.url.clone());
                self.client_info().serve(transport).await?
            }
        };

        Ok(Arc::new(client))
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let client = self.connect().await?;
        let tools = client.list_tools(None).await?;
        Ok(tools
            .tools
            .into_iter()
            .filter_map(|tool| {
                Some(
                    ToolDefinition::new(tool.name)
                        .description(tool.description.unwrap_or_default())
                        .input_schema(
                            serde_json::from_value::<RootSchema>(Value::Object(
                                tool.input_schema.as_ref().clone(),
                            ))
                            .ok()?,
                        ),
                )
            })
            .collect())
    }

    async fn call(&self, tool_name: &ToolName, input: &Value) -> anyhow::Result<ToolOutput> {
        let client = self.connect().await?;
        let result = client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(tool_name.to_string()),
                arguments: if let Value::Object(args) = input {
                    Some(args.clone())
                } else {
                    None
                },
            })
            .await?;

        let tool_contents: Vec<ToolOutput> = result
            .content
            .into_iter()
            .map(|content| match content.raw {
                rmcp::model::RawContent::Text(raw_text_content) => {
                    Ok(ToolOutput::text(raw_text_content.text))
                }
                rmcp::model::RawContent::Image(raw_image_content) => Ok(ToolOutput::image(
                    Image::new_base64(raw_image_content.data, raw_image_content.mime_type.as_str()),
                )),
                rmcp::model::RawContent::Resource(_) => {
                    Err(Error::UnsupportedMcpResponse("Resource").into())
                }

                rmcp::model::RawContent::Audio(_) => {
                    Err(Error::UnsupportedMcpResponse("Audio").into())
                }
            })
            .collect::<anyhow::Result<Vec<ToolOutput>>>()?;

        Ok(ToolOutput::from(tool_contents.into_iter())
            .is_error(result.is_error.unwrap_or_default()))
    }

    async fn attempt_with_retry<T, F>(&self, call: impl Fn() -> F) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        call.retry(
            ExponentialBuilder::default()
                .with_max_times(5)
                .with_jitter(),
        )
        .when(|err| {
            let is_transport = err
                .downcast_ref::<rmcp::ServiceError>()
                .map(|e| {
                    matches!(
                        e,
                        rmcp::ServiceError::TransportSend(_) | rmcp::ServiceError::TransportClosed
                    )
                })
                .unwrap_or(false);

            if is_transport {
                self.client.write().unwrap().take();
            }

            is_transport
        })
        .await
    }
}

#[async_trait::async_trait]
impl McpClientInfra for ForgeMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.attempt_with_retry(|| self.list()).await
    }

    async fn call(&self, tool_name: &ToolName, input: Value) -> anyhow::Result<ToolOutput> {
        self.attempt_with_retry(|| self.call(tool_name, &input))
            .await
    }
}

#[cfg(test)]
mod test_mcp {
    use std::sync::Arc;

    use rmcp::model::*;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use rmcp::transport::streamable_http_server::StreamableHttpService;
    use rmcp::{schemars, tool, Error as McpError, ServerHandler};
    use tokio::sync::Mutex;

    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    pub struct SumRequest {
        pub a: i32,
        pub b: i32,
    }

    #[derive(Clone)]
    pub struct TestMcpServer {
        counter: Arc<Mutex<i32>>,
    }

    #[tool(tool_box)]
    impl TestMcpServer {
        pub fn new() -> Self {
            Self { counter: Arc::new(Mutex::new(0)) }
        }

        #[tool(description = "Increment the counter by 1")]
        async fn increment(&self) -> Result<CallToolResult, McpError> {
            let mut counter = self.counter.lock().await;
            *counter += 1;
            Ok(CallToolResult::success(vec![Content::text(
                counter.to_string(),
            )]))
        }

        #[tool(description = "Get the current counter value")]
        async fn get_value(&self) -> Result<CallToolResult, McpError> {
            let counter = self.counter.lock().await;
            Ok(CallToolResult::success(vec![Content::text(
                counter.to_string(),
            )]))
        }

        #[tool(description = "Say hello to the client")]
        fn say_hello(&self) -> Result<CallToolResult, McpError> {
            Ok(CallToolResult::success(vec![Content::text(
                "Hello from streaming MCP server!",
            )]))
        }

        #[tool(description = "Calculate the sum of two numbers")]
        fn sum(
            &self,
            #[tool(aggr)] SumRequest { a, b }: SumRequest,
        ) -> Result<CallToolResult, McpError> {
            Ok(CallToolResult::success(vec![Content::text(
                (a + b).to_string(),
            )]))
        }
    }

    #[tool(tool_box)]
    impl ServerHandler for TestMcpServer {
        fn get_info(&self) -> ServerInfo {
            ServerInfo {
                protocol_version: ProtocolVersion::V_2024_11_05,
                capabilities: ServerCapabilities::builder()
                    .enable_tools()
                    .build(),
                server_info: Implementation::from_build_env(),
                instructions: Some("Test MCP server for streaming HTTP functionality. Provides counter and calculation tools.".to_string()),
            }
        }
    }

    use forge_domain::{McpServerConfig, ToolName};
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    use crate::mcp_client::ForgeMcpClient;

    async fn start_test_server_background(port: u16) -> anyhow::Result<()> {
        let bind_address = format!("127.0.0.1:{}", port);
        let service = StreamableHttpService::new(
            || TestMcpServer::new(),
            LocalSessionManager::default().into(),
            Default::default(),
        );

        let router = axum::Router::new().nest_service("/mcp", service);
        let tcp_listener = tokio::net::TcpListener::bind(&bind_address).await?;

        tracing::debug!("Starting test MCP server on {}", bind_address);

        tokio::spawn(async move {
            axum::serve(tcp_listener, router).await.unwrap();
        });

        // Give the server a moment to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_mcp_client_streamable_http() {
        let _ = tracing_subscriber::fmt::try_init();

        let fixture = async {
            // Start test server
            start_test_server_background(8002).await?;

            // Create client config for streamable HTTP
            let config = McpServerConfig::StreamableHttp(forge_domain::McpStreamableHttpServer {
                url: "http://127.0.0.1:8002/mcp".to_string(),
            });

            let client = ForgeMcpClient::new(config);

            Ok::<_, anyhow::Error>(client)
        };

        let client = timeout(Duration::from_secs(5), fixture)
            .await
            .unwrap()
            .unwrap();

        // Test say_hello tool
        let hello_result = timeout(
            Duration::from_secs(5),
            client.call(&ToolName::new("say_hello"), &json!({})),
        )
        .await;

        assert!(hello_result.is_ok());
        let hello_output = hello_result.unwrap().unwrap();
        let actual = hello_output.as_str().unwrap_or("");
        let expected = "Hello from streaming MCP server!";
        assert!(actual.contains(expected));

        // Test sum tool
        let sum_result = timeout(
            Duration::from_secs(5),
            client.call(&ToolName::new("sum"), &json!({"a": 5, "b": 3})),
        )
        .await;

        assert!(sum_result.is_ok());
        let sum_output = sum_result.unwrap().unwrap();
        let actual = sum_output.as_str().unwrap_or("");
        let expected = "8";
        assert!(actual.contains(expected));

        // Test counter functionality
        let increment_result = timeout(
            Duration::from_secs(5),
            client.call(&ToolName::new("increment"), &json!({})),
        )
        .await;

        assert!(increment_result.is_ok());
        let increment_output = increment_result.unwrap().unwrap();
        let actual = increment_output.as_str().unwrap_or("");
        let expected = "1";
        assert!(actual.contains(expected));

        // Test get_value tool
        let value_result = timeout(
            Duration::from_secs(5),
            client.call(&ToolName::new("get_value"), &json!({})),
        )
        .await;

        assert!(value_result.is_ok());
        let value_output = value_result.unwrap().unwrap();
        let actual = value_output.as_str().unwrap_or("");
        let expected = "1";
        assert!(actual.contains(expected));
    }
}
