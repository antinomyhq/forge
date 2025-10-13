use std::borrow::Cow;
use std::future::Future;
use std::sync::{Arc, RwLock};

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{Image, McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use forge_services::McpClientInfra;
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation};
use rmcp::service::RunningService;
use rmcp::transport::{SseClientTransport, TokioChildProcess};
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use schemars::schema::RootSchema;
use serde_json::Value;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::error::Error;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

type RmcpClient = RunningService<RoleClient, ForgeClientHandler>;

/// Handler for MCP client notifications (logging, progress, elicitation)
#[derive(Default, Clone)]
pub struct ForgeClientHandler;

impl ClientHandler for ForgeClientHandler {
    fn get_info(&self) -> ClientInfo {
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
}

#[derive(Clone)]
pub struct ForgeMcpClient {
    client: Arc<RwLock<Option<Arc<RmcpClient>>>>,
    config: McpServerConfig,
    server_name: String,
    handler: Arc<ForgeClientHandler>,
}

impl ForgeMcpClient {
    /// Creates a new MCP client with the given configuration and server name.
    pub fn new(config: McpServerConfig, server_name: String) -> Self {
        let handler = Arc::new(ForgeClientHandler);

        Self { client: Default::default(), config, server_name, handler }
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
        debug!(server = %self.server_name, "Creating MCP server connection");

        let client = match &self.config {
            McpServerConfig::Stdio(stdio) => {
                let mut cmd = Command::new(&stdio.command);
                cmd.args(&stdio.args);

                for (key, value) in &stdio.env {
                    cmd.env(key, value);
                }

                cmd.stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true);

                // Use builder pattern to capture stderr
                let (transport, stderr) = TokioChildProcess::builder(cmd)
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| Error::McpConnectionFailed {
                        server: self.server_name.clone(),
                        reason: format!("Failed to spawn process: {}", e),
                    })?;

                // Spawn task to capture and log stderr from the MCP server
                if let Some(stderr) = stderr {
                    let server_name = self.server_name.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncBufReadExt;
                        let reader = tokio::io::BufReader::new(stderr);
                        let mut lines = reader.lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            // Route MCP server stderr to our logging system
                            // Use warn level since stderr typically indicates issues
                            warn!(
                                server = %server_name,
                                source = "stderr",
                                message = %line,
                                "MCP server stderr output"
                            );
                        }
                    });
                }

                (*self.handler)
                    .clone()
                    .serve(transport)
                    .await
                    .map_err(|e| Error::McpConnectionFailed {
                        server: self.server_name.clone(),
                        reason: format!("Stdio connection handshake failed: {}", e),
                    })?
            }
            McpServerConfig::Sse(sse) => {
                let transport = SseClientTransport::start(sse.url.clone())
                    .await
                    .map_err(|e| Error::McpConnectionFailed {
                        server: self.server_name.clone(),
                        reason: format!("Failed to start SSE transport: {}", e),
                    })?;

                (*self.handler)
                    .clone()
                    .serve(transport)
                    .await
                    .map_err(|e| Error::McpConnectionFailed {
                        server: self.server_name.clone(),
                        reason: format!("SSE connection handshake failed: {}", e),
                    })?
            }
        };

        info!(server = %self.server_name, "MCP server connection established");

        Ok(Arc::new(client))
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        debug!(server = %self.server_name, "Listing MCP server tools");

        let client = self.connect().await?;
        let tools = client
            .list_tools(None)
            .await
            .map_err(|e| Error::McpServer {
                server: self.server_name.clone(),
                message: format!("Failed to list tools: {}", e),
            })?;

        let mut result = Vec::new();
        for tool in tools.tools {
            match serde_json::from_value::<RootSchema>(Value::Object(
                tool.input_schema.as_ref().clone(),
            )) {
                Ok(schema) => {
                    result.push(
                        ToolDefinition::new(tool.name)
                            .description(tool.description.unwrap_or_default())
                            .input_schema(schema),
                    );
                }
                Err(e) => {
                    warn!(
                        server = %self.server_name,
                        tool = %tool.name,
                        error = ?e,
                        "Failed to parse tool schema"
                    );
                }
            }
        }

        debug!(
            server = %self.server_name,
            tool_count = result.len(),
            "Listed MCP server tools"
        );

        Ok(result)
    }

    async fn call(&self, tool_name: &ToolName, input: &Value) -> anyhow::Result<ToolOutput> {
        debug!(
            server = %self.server_name,
            tool_name = %tool_name,
            "Calling MCP server tool"
        );

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
            .await
            .map_err(|e| Error::McpToolExecutionFailed {
                server: self.server_name.clone(),
                tool: tool_name.to_string(),
                reason: format!("Tool call failed: {}", e),
            })?;

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
                    warn!(
                        server = %self.server_name,
                        tool_name = %tool_name,
                        "Resource content type not fully supported, skipping"
                    );
                    Err(Error::UnsupportedMcpResponse("Resource").into())
                }
                rmcp::model::RawContent::ResourceLink(_) => {
                    warn!(
                        server = %self.server_name,
                        tool_name = %tool_name,
                        "ResourceLink content type not fully supported, skipping"
                    );
                    Err(Error::UnsupportedMcpResponse("ResourceLink").into())
                }
                rmcp::model::RawContent::Audio(_) => {
                    warn!(
                        server = %self.server_name,
                        tool_name = %tool_name,
                        "Audio content type not fully supported, skipping"
                    );
                    Err(Error::UnsupportedMcpResponse("Audio").into())
                }
            })
            .collect::<anyhow::Result<Vec<ToolOutput>>>()?;

        let output = ToolOutput::from(tool_contents.into_iter())
            .is_error(result.is_error.unwrap_or_default());

        debug!(
            server = %self.server_name,
            tool_name = %tool_name,
            is_error = output.is_error,
            "MCP server tool call completed"
        );

        Ok(output)
    }

    async fn attempt_with_retry<T, F>(&self, call: impl Fn() -> F) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        let mut attempt = 0;

        call.retry(
            ExponentialBuilder::default()
                .with_max_times(5)
                .with_jitter(),
        )
        .when(|err| {
            attempt += 1;

            // Check if this is a transport error that should trigger retry
            let is_transport = err
                .downcast_ref::<rmcp::ServiceError>()
                .map(|e| {
                    matches!(
                        e,
                        rmcp::ServiceError::TransportSend(_) | rmcp::ServiceError::TransportClosed
                    )
                })
                .unwrap_or(false)
                || err
                    .downcast_ref::<Error>()
                    .map(|e| {
                        // Our custom errors that wrap transport failures should also trigger retry
                        matches!(e, Error::McpConnectionFailed { .. })
                    })
                    .unwrap_or(false);

            if is_transport {
                warn!(
                    server = %self.server_name,
                    attempt = attempt,
                    error = ?err,
                    "Retrying MCP server operation due to transport error"
                );

                // Clear client to force reconnection
                match self.client.write() {
                    Ok(mut guard) => {
                        if guard.take().is_some() {
                            debug!(
                                server = %self.server_name,
                                "Cleared cached MCP client, will reconnect on retry"
                            );
                        }
                    }
                    Err(poisoned) => {
                        warn!(
                            server = %self.server_name,
                            "RwLock poisoned while clearing MCP client, recovering"
                        );
                        poisoned.into_inner().take();
                    }
                }
            } else {
                // Log non-transport errors at error level since they won't be retried
                error!(
                    server = %self.server_name,
                    error = ?err,
                    "MCP server operation failed with non-retryable error"
                );
            }

            is_transport
        })
        .await
        .inspect_err(|err| {
            if attempt > 1 {
                error!(
                    server = %self.server_name,
                    attempts = attempt,
                    error = ?err,
                    "MCP server retry attempts exhausted"
                );
            }
        })
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
