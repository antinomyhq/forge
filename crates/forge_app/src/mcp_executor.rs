use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::{TitleFormat, ToolCallContext, ToolCallFull, ToolName, ToolOutput, ToolValue};
use forge_template::Element;

use crate::truncation::{TruncationResult, truncate_mcp_output};
use crate::{EnvironmentService, FsWriteService, McpService};

pub struct McpExecutor<S> {
    services: Arc<S>,
}

impl<S: McpService + EnvironmentService + FsWriteService> McpExecutor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        context
            .send_title(TitleFormat::info("MCP").sub_title(input.name.as_str()))
            .await?;

        let output = self.services.execute_mcp(input).await?;
        self.truncate_if_needed(output).await
    }

    pub async fn contains_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        let mcp_servers = self.services.get_mcp_servers().await?;
        Ok(mcp_servers
            .get_servers()
            .values()
            .any(|tools| tools.iter().any(|tool| tool.name == *tool_name)))
    }

    /// Creates a temporary file with the given content.
    /// Uses the same pattern as tool_executor for consistency.
    async fn create_temp_file(
        &self,
        prefix: &str,
        ext: &str,
        content: &str,
    ) -> anyhow::Result<PathBuf> {
        let path = tempfile::Builder::new()
            .disable_cleanup(true)
            .prefix(prefix)
            .suffix(ext)
            .tempfile()?
            .into_temp_path()
            .to_path_buf();
        self.services
            .write(
                path.to_string_lossy().to_string(),
                content.to_string(),
                true,
            )
            .await?;
        Ok(path)
    }

    /// Truncates MCP output if text content exceeds the limit.
    /// Writes full content to temp file when truncation occurs.
    async fn truncate_if_needed(&self, output: ToolOutput) -> anyhow::Result<ToolOutput> {
        let env = self.services.get_environment();
        let limit = env.mcp_truncation_limit;
        match truncate_mcp_output(output, limit)? {
            TruncationResult::Unchanged(output) => Ok(output),
            TruncationResult::Truncated {
                mut truncated_values,
                full_json,
                total_size,
                limit,
                is_error,
            } => {
                // Write full content to temp file
                let temp_path = self
                    .create_temp_file("forge_mcp_", ".json", &full_json)
                    .await?;

                // Add truncation notice
                let notice = Element::new("truncated").text(format!(
                    "Content truncated to {} chars (total: {} chars).\n Full content:\n{}",
                    limit,
                    total_size,
                    temp_path.display()
                ));
                truncated_values.push(ToolValue::Text(notice.render()));

                Ok(ToolOutput { is_error, values: truncated_values })
            }
        }
    }
}
