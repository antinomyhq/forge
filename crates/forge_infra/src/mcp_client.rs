use std::borrow::Cow;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context;
use backon::{ExponentialBuilder, Retryable};
use forge_domain::{McpServerConfig, ToolDefinition, ToolName};
use forge_services::McpClient;
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::schemars::schema::RootSchema;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::Mutex;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

pub struct ForgeMcpClient {
    client: Arc<Mutex<Option<RunningService<RoleClient, InitializeRequestParam>>>>,
    config: McpServerConfig,
    reconnect: Arc<AtomicBool>,
}

impl ForgeMcpClient {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            client: Default::default(),
            config,
            reconnect: Arc::new(AtomicBool::new(false)),
        }
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
    async fn connect(&self) -> anyhow::Result<()> {
        let mut guard = self.client.lock().await;
        if guard.is_none() || self.reconnect.load(Ordering::SeqCst) {
            self.reconnect.store(false, Ordering::SeqCst);

            let client = match &self.config {
                McpServerConfig::Stdio(stdio) => {
                    let command = stdio
                        .command
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("Command not specified"))?;
                    let mut cmd = Command::new(command);

                    if let Some(env) = &stdio.env {
                        for (key, value) in env {
                            cmd.env(key, value);
                        }
                    }

                    cmd.stdin(std::process::Stdio::inherit())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped());
                    self.client_info()
                        .serve(TokioChildProcess::new(cmd.args(&stdio.args))?)
                        .await?
                }
                McpServerConfig::Sse(sse) => {
                    let url = sse
                        .url
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("URL not specified"))?;
                    let transport = rmcp::transport::SseTransport::start(url).await?;
                    self.client_info().serve(transport).await?
                }
            };
            *guard = Some(client);
        }
        Ok(())
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.connect().await?;
        let client = self.client.lock().await;
        let client = client.as_ref().context("Client is not running")?;
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

    async fn call(&self, tool_name: &ToolName, input: &Value) -> anyhow::Result<String> {
        self.connect().await?;
        let client = self.client.lock().await;
        let client = client.as_ref().context("Client is not running")?;

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

        let content = serde_json::to_string(&result.content)?;

        if result.is_error.unwrap_or_default() {
            anyhow::bail!("{}", content)
        } else {
            Ok(content)
        }
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

            if is_transport {
                self.reconnect.store(true, Ordering::SeqCst);
            }

            is_transport
        })
        .await
    }
}

#[async_trait::async_trait]
impl McpClient for ForgeMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.attempt_with_retry(|| self.list()).await
    }

    async fn call(&self, tool_name: &ToolName, input: Value) -> anyhow::Result<String> {
        self.attempt_with_retry(|| self.call(tool_name, &input))
            .await
    }
}
