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

#[async_trait::async_trait]
impl<T: McpClient> ExecutableTool for McpTool<T> {
    type Input = serde_json::Value;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        context
            .send_text(TitleFormat::debug("MCP").sub_title(self.tool_name.as_str()))
            .await?;

        // Create a retry strategy based on the retry_config
        let retry_strategy = ExponentialBackoff::from_millis(self.retry_config.initial_backoff_ms)
            .factor(self.retry_config.backoff_factor)
            .take(self.retry_config.max_retry_attempts)
            .map(jitter);

        // Retry the operation with exponential backoff
        RetryIf::spawn(
            retry_strategy,
            || {
                let client = Arc::clone(&self.client);
                let tool_name = self.tool_name.clone();
                let input = input.clone();

                async move {
                    client
                        .call_tool(&tool_name, input)
                        .await
                        .context("Failed to call MCP tool")
                }
            },
            |err: &anyhow::Error| {
                let is_transport_error = err
                    .downcast_ref::<rmcp::ServiceError>()
                    .map(|e| matches!(e, rmcp::ServiceError::Transport(_)))
                    .unwrap_or(false);

                if is_transport_error {
                    // Log the retry attempt
                    debug!(
                        tool_name = %self.tool_name,
                        server_name = %self.server_name,
                        error = %err,
                        "Retrying MCP connection due to transport error"
                    );

                    // Attempt to reconnect - need to handle this as a future
                    futures::executor::block_on(async {
                        match self.client.reconnect().await {
                            Ok(_) => true, // Reconnect successful, retry the operation
                            Err(reconnect_err) => {
                                debug!(
                                    tool_name = %self.tool_name,
                                    server_name = %self.server_name,
                                    error = %reconnect_err,
                                    "Failed to reconnect to MCP server"
                                );
                                false // Don't retry if reconnect failed
                            }
                        }
                    })
                } else {
                    false // Don't retry non-transport errors
                }
            },
        )
        .await
        .context(format!(
            "Failed to connect to MCP server: {}",
            self.server_name
        ))
    }
}
