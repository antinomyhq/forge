use forge_domain::{ExecutableTool, Executor, NamedTool, ToolDescription, ToolName, ToolOutput};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShellInput {
    /// The content to be passed in input/stdin.
    pub input: String,
    /// Non-zero timeout in seconds
    /// to determine how much amount of time to wait for an output to avoid
    /// freezing the program.
    pub timeout: Option<u64>,
}

/// Pass input to an actively running shell command.
///
/// This tool call allows the LLM to provide standard input (stdin) to a
/// shell command that is currently executing. It is useful for interactive
/// commands that require additional input after execution has started.
///
/// # Usage
/// The tool interacts with a running shell process, ensuring that user-provided
/// input is safely passed to the appropriate command. This is particularly
/// useful when dealing with prompts requiring user input (e.g., password
/// prompts, interactive scripts).
#[derive(ToolDescription)]
pub struct ShellInputTool;

impl NamedTool for ShellInputTool {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_shell_input")
    }
}

#[async_trait::async_trait]
impl ExecutableTool for ShellInputTool {
    type Input = ShellInput;

    async fn call(
        &self,
        input: Self::Input,
        executor: Option<&mut Executor>,
    ) -> anyhow::Result<ToolOutput> {
        let executor = executor.ok_or_else(|| anyhow::anyhow!("Executor is required"))?;
        let value = executor.execute(Some(input.input), input.timeout).await?;
        Ok(ToolOutput::Text(value))
    }
}
