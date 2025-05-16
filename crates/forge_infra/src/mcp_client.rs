use std::borrow::Cow;
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{ToolDefinition, ToolName};
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

enum Connector {
    Stdio {
        command: String,
        env: std::collections::BTreeMap<String, String>,
        args: Vec<String>,
    },
    Sse {
        url: String,
    },
}

impl Connector {
    fn client_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation { name: "Forge".to_string(), version: VERSION.to_string() },
        }
    }

    async fn connect(&self) -> anyhow::Result<RunningService<RoleClient, InitializeRequestParam>> {
        match self {
            Connector::Stdio { command, env, args } => {
                let mut command = Command::new(command);

                for (key, value) in env {
                    command.env(key, value);
                }

                command
                    .stdin(std::process::Stdio::inherit())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                let client = self
                    .client_info()
                    .serve(TokioChildProcess::new(command.args(args))?)
                    .await?;

                Ok(client)
            }
            Connector::Sse { url } => {
                let transport = rmcp::transport::SseTransport::start(url).await?;
                let client = self.client_info().serve(transport).await?;
                Ok(client)
            }
        }
    }
}

pub struct ForgeMcpClient {
    client: Arc<Mutex<Option<RunningService<RoleClient, InitializeRequestParam>>>>,
    connector: Connector,
}

impl ForgeMcpClient {
    fn new(connector: Connector) -> Self {
        Self { client: Default::default(), connector }
    }
    pub fn new_stdio(
        command: String,
        env: std::collections::BTreeMap<String, String>,
        args: Vec<String>,
    ) -> Self {
        Self::new(Connector::Stdio { command, env, args })
    }

    pub fn new_sse(url: String) -> Self {
        Self::new(Connector::Sse { url })
    }

    /// Connects to the MCP server. If `force` is true, it will reconnect even if already connected.
    async fn connect(&self, force: bool) -> anyhow::Result<()> {
        let mut client = self.client.lock().await;
        if client.is_none() || force {
            *client = Some(self.connector.connect().await?);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl McpClient for ForgeMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.connect(false).await?;
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

    async fn call(&self, tool_name: &ToolName, input: Value) -> anyhow::Result<String> {
        self.connect(false).await?;
        let client = self.client.lock().await;
        let client = client.as_ref().context("Client is not running")?;

        let result = client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(tool_name.to_string()),
                arguments: if let Value::Object(args) = input {
                    Some(args)
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

    async fn reconnect(&self) -> anyhow::Result<()> {
        self.connect(true).await?;
        Ok(())
    }
}
