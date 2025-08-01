use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsString;
use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{Image, McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use forge_services::McpClientInfra;
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::schemars::schema::RootSchema;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;

use crate::error::Error;
use crate::mcp_stdio_client::McpClient;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

type RmcpClient = RunningService<RoleClient, InitializeRequestParam>;

#[derive(Clone)]
enum ClientType {
    Stdio(Arc<McpClient>),
    Sse(Arc<RmcpClient>),
}

#[derive(Clone)]
pub struct ForgeMcpClient {
    client: Arc<RwLock<Option<ClientType>>>,
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

    async fn connect(&self) -> anyhow::Result<ClientType> {
        if let Some(client) = self.get_client() {
            Ok(client)
        } else {
            let client = self.create_connection().await?;
            self.set_client(client.clone());
            Ok(client)
        }
    }

    fn get_client(&self) -> Option<ClientType> {
        let guard = self.client.read().unwrap();
        guard.clone()
    }

    fn set_client(&self, client: ClientType) {
        let mut guard = self.client.write().unwrap();
        *guard = Some(client);
    }

    async fn create_connection(&self) -> anyhow::Result<ClientType> {
        match &self.config {
            McpServerConfig::Stdio(stdio) => {
                let program = OsString::from(&stdio.command);
                let args: Vec<OsString> = stdio.args.iter().map(OsString::from).collect();
                let env: HashMap<String, String> = stdio
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                let mcp_client = McpClient::new_stdio_client(program, args, Some(env)).await?;

                let init_params = InitializeRequestParam {
                    protocol_version: Default::default(),
                    capabilities: Default::default(),
                    client_info: Implementation {
                        name: "Forge".to_string(),
                        version: VERSION.to_string(),
                    },
                };

                mcp_client
                    .initialize(init_params, None, Some(Duration::from_secs(30)))
                    .await?;

                Ok(ClientType::Stdio(Arc::new(mcp_client)))
            }
            McpServerConfig::Sse(sse) => {
                let transport = rmcp::transport::SseTransport::start(sse.url.clone()).await?;
                let client = self.client_info().serve(transport).await?;
                Ok(ClientType::Sse(Arc::new(client)))
            }
        }
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let client = self.connect().await?;
        let tools = match client {
            ClientType::Stdio(stdio_client) => {
                stdio_client
                    .list_tools(None, Some(Duration::from_secs(30)))
                    .await?
            }
            ClientType::Sse(sse_client) => sse_client.list_tools(None).await?,
        };

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
        let result = match client {
            ClientType::Stdio(stdio_client) => {
                let arguments = if let Value::Object(args) = input {
                    Some(serde_json::Value::Object(args.clone()))
                } else {
                    None
                };
                stdio_client
                    .call_tool(
                        tool_name.to_string(),
                        arguments,
                        Some(Duration::from_secs(30)),
                    )
                    .await?
            }
            ClientType::Sse(sse_client) => {
                sse_client
                    .call_tool(CallToolRequestParam {
                        name: Cow::Owned(tool_name.to_string()),
                        arguments: if let Value::Object(args) = input {
                            Some(args.clone())
                        } else {
                            None
                        },
                    })
                    .await?
            }
        };

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
                .map(|e| matches!(e, rmcp::ServiceError::Transport(_)))
                .unwrap_or(false);

            let is_stdio_error = err
                .to_string()
                .contains("failed to send message to writer task")
                || err.to_string().contains("response channel closed")
                || err.to_string().contains("request timed out");

            if is_transport || is_stdio_error {
                self.client.write().unwrap().take();
            }

            is_transport || is_stdio_error
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
