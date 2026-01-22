use std::path::PathBuf;
use std::sync::Arc;

use forge_domain::{TitleFormat, ToolCallContext, ToolCallFull, ToolName, ToolOutput, ToolValue};
use forge_template::Element;

use crate::truncation::truncate_text_if_needed;
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

    /// Truncates MCP output if any text value exceeds the limit.
    /// Each truncated value gets its own temp file and XML wrapper.
    async fn truncate_if_needed(&self, output: ToolOutput) -> anyhow::Result<ToolOutput> {
        let limit = self.services.get_environment().mcp_truncation_limit;
        let mut new_values = Vec::with_capacity(output.values.len());
        for value in output.values {
            match value {
                ToolValue::Text(text) => {
                    match truncate_text_if_needed(&text, limit) {
                        None => {
                            // Not truncated, keep as-is
                            new_values.push(ToolValue::Text(text));
                        }
                        Some(truncated) => {
                            // Write full content to temp file
                            let temp_path = self
                                .create_temp_file("forge_mcp_", ".txt", &truncated.full_text)
                                .await?;

                            // Wrap in XML with metadata
                            let xml = Element::new("mcp_output")
                                .attr("original_size", truncated.original_size)
                                .attr("limit", limit)
                                .attr("file_path", temp_path.display())
                                .cdata(&truncated.content);

                            new_values.push(ToolValue::Text(xml.render()));
                        }
                    }
                }
                other => {
                    // Non-text values pass through unchanged
                    new_values.push(other);
                }
            }
        }
        Ok(ToolOutput { values: new_values, is_error: output.is_error })
    }
}
