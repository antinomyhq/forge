use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::{TitleFormat, ToolCallContext, ToolCallFull, ToolName, ToolOutput};
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
        Ok(self
            .truncate_if_needed(output.clone())
            .await
            .unwrap_or(output))
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
                truncated_values,
                full_json,
                total_size,
                limit,
                is_error,
            } => {
                // Write full content to temp file
                let temp_path = self
                    .create_temp_file("forge_mcp_", ".json", &full_json)
                    .await?;

                // Wrap truncated values in XML structure with truncation notice
                let mut elm = Element::new("mcp_output")
                    .attr("total_size", total_size)
                    .attr("limit", limit)
                    .attr("file_path", temp_path.display());

                // Add all truncated values as content
                for value in truncated_values {
                    if let Some(text) = value.as_str() {
                        elm = elm.append(Element::new("content").cdata(text));
                    }
                }

                // Add truncation notice as nested element with metadata attributes
                elm = elm.append(
                    Element::new("truncated")
                        .attr("limit", limit)
                        .attr("total_size", total_size)
                        .text("Content is truncated. Full content is available at the specified path."),
                );

                Ok(ToolOutput::text(elm).is_error(is_error))
            }
        }
    }
}
