use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context as _;
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, RetryConfig, ToolCallContext, ToolName};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;
use tracing::debug;

use crate::McpClient;

pub struct McpTool<T> {
    pub client: Arc<T>,
    pub tool_name: ToolName,
    pub server_name: String,
    pub retry_config: RetryConfig,
}

impl<T> McpTool<T> {
    pub fn new(
        server_name: impl ToString,
        tool_name: ToolName,
        client: Arc<T>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            client,
            tool_name,
            server_name: server_name.to_string(),
            retry_config: RetryConfig::default(),
        })
    }
}

impl<T: McpClient> McpTool<T> {
    async fn call_tool(
        &self,
        tool_name: &ToolName,
        input: serde_json::Value,
        force_reconnect: bool,
    ) -> anyhow::Result<String> {
        if force_reconnect {
            match self.client.reconnect().await {
                Ok(_) => debug!(
                    tool_name = %tool_name,
                    server_name = %self.server_name,
                    "Successfully reconnected to MCP server"
                ),
                Err(err) => debug!(
                    tool_name = %tool_name,
                    server_name = %self.server_name,
                    error = %err,
                    "Failed to reconnect to MCP server"
                ),
            }
        }

        self.client
            .call(tool_name, input)
            .await
            .context("Failed to call MCP tool")
    }
    fn should_retry(err: &anyhow::Error, is_retry: Arc<AtomicBool>) -> bool {
        let retry = err
            .downcast_ref::<rmcp::ServiceError>()
            .map(|e| matches!(e, rmcp::ServiceError::Transport(_)))
            .unwrap_or(false);
        if retry {
            is_retry.store(true, Ordering::Relaxed);
        }
        retry
    }
}

#[async_trait::async_trait]
impl<T: McpClient> ExecutableTool for McpTool<T> {
    type Input = serde_json::Value;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        context
            .send_text(TitleFormat::info("MCP").sub_title(self.tool_name.as_str()))
            .await?;

        let retry_strategy = ExponentialBackoff::from_millis(self.retry_config.initial_backoff_ms)
            .factor(self.retry_config.backoff_factor)
            .take(self.retry_config.max_retry_attempts)
            .map(jitter);

        let is_retry = Arc::new(AtomicBool::new(false));

        RetryIf::spawn(
            retry_strategy,
            || {
                self.call_tool(
                    &self.tool_name,
                    input.clone(),
                    is_retry.clone().load(Ordering::SeqCst),
                )
            },
            |err: &anyhow::Error| Self::should_retry(err, is_retry.clone()),
        )
        .await
        .context(format!(
            "Failed to connect to MCP server: {}",
            self.server_name
        ))
    }
}
