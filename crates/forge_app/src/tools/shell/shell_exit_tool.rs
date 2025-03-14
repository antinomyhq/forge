use forge_domain::{ExecutableTool, Executor, NamedTool, ToolDescription, ToolName, ToolOutput};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellExitInput {}

/// Terminate an actively running shell command.
///
/// This tool call allows the LLM to send a termination signal to a shell
/// command that is currently executed. It is useful for stopping interactive or
/// long-running processes initiated via the `Shell` tool.
#[derive(ToolDescription)]
pub struct ShellExitTool;

impl NamedTool for ShellExitTool {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_shell_exit")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for ShellExitTool {
    type Input = ShellExitInput;

    async fn call(
        &self,
        _: Self::Input,
        executor: Option<&mut Executor>,
    ) -> anyhow::Result<ToolOutput> {
        let executor = executor.ok_or_else(|| anyhow::anyhow!("Executor is required"))?;
        executor.exit().await?;
        Ok(ToolOutput::Text("Shell command terminated".to_string()))
    }
}
