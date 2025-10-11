use std::borrow::Cow;
use std::future::Future;
use std::sync::{Arc, RwLock};

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{Image, McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use forge_services::McpClientInfra;
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::service::RunningService;
use rmcp::transport::{SseClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use schemars::schema::RootSchema;
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
            client_info: Implementation {
                name: "Forge".to_string(),
                version: VERSION.to_string(),
                icons: None,
                title: None,
                website_url: None,
            },
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

                for (key, value) in &stdio.env {
                    cmd.env(key, value);
                }

                cmd.args(&stdio.args)
                    .stdin(std::process::Stdio::inherit())
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
                rmcp::model::RawContent::ResourceLink(_) => {
                    Err(Error::UnsupportedMcpResponse("ResourceLink").into())
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
