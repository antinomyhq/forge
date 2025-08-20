use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use console::style;
use forge_domain::{
    Agent, AgentInput, ChatResponse, ToolCallContext, ToolCallFull, ToolDefinition, ToolName,
    ToolOutput, ToolResult, Tools, ToolsDiscriminants,
};
use strum::IntoEnumIterator;
use tokio::time::timeout;

use crate::agent_executor::AgentExecutor;
use crate::error::Error;
use crate::mcp_executor::McpExecutor;
use crate::tool_executor::ToolExecutor;
use crate::{EnvironmentService, McpService, Services};

pub struct ToolRegistry<S> {
    tool_executor: ToolExecutor<S>,
    agent_executor: AgentExecutor<S>,
    mcp_executor: McpExecutor<S>,
    tool_timeout: Duration,
}

impl<S: Services> ToolRegistry<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self {
            tool_executor: ToolExecutor::new(services.clone()),
            agent_executor: AgentExecutor::new(services.clone()),
            mcp_executor: McpExecutor::new(services.clone()),
            tool_timeout: Duration::from_secs(services.get_environment().tool_timeout),
        }
    }

    async fn call_with_timeout<F, Fut>(
        &self,
        tool_name: &ToolName,
        future: F,
    ) -> anyhow::Result<ToolOutput>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<ToolOutput>>,
    {
        timeout(self.tool_timeout, future())
            .await
            .context(Error::CallTimeout {
                timeout: self.tool_timeout.as_secs() / 60,
                tool_name: tool_name.clone(),
            })?
    }

    async fn call_inner(
        &self,
        agent: &Agent,
        input: ToolCallFull,
        context: &mut ToolCallContext<'_>,
    ) -> anyhow::Result<ToolOutput> {
        Self::validate_tool_call(agent, &input.name)?;

        tracing::info!(tool_name = %input.name, arguments = %input.arguments.clone().into_string(), "Executing tool call");
        let tool_name = input.name.clone();

        // First, try to call a Forge tool
        if Tools::contains(&input.name) {
            // Only apply timeout to specific tools (search, shell, fetch) to avoid
            // timing out interactive operations like permission requests
            if Self::should_apply_timeout(&tool_name) {
                self.call_with_timeout(&tool_name, || self.tool_executor.execute(input, context))
                    .await
            } else {
                // Execute without timeout for interactive operations
                self.tool_executor.execute(input, context).await
            }
        } else if self.agent_executor.contains_tool(&input.name).await? {
            // Handle agent delegation tool calls
            let agent_input = AgentInput::try_from(&input)?;
            // NOTE: Agents should not timeout
            self.agent_executor
                .execute(input.name.to_string(), agent_input.task, context)
                .await
        } else if self.mcp_executor.contains_tool(&input.name).await? {
            // MCP tools should always have timeouts
            let output = self
                .call_with_timeout(&tool_name, || self.mcp_executor.execute(input, context))
                .await?;
            let text = output
                .values
                .iter()
                .filter_map(|output| output.as_str())
                .fold(String::new(), |mut a, b| {
                    a.push('\n');
                    a.push_str(b);
                    a
                });
            if !text.trim().is_empty() {
                let text = style(text).cyan().dim().to_string();
                context
                    .send(ChatResponse::Text { text, is_complete: true, is_md: false })
                    .await?;
            }
            Ok(output)
        } else {
            Err(Error::NotFound(input.name).into())
        }
    }

    pub async fn call(
        &self,
        agent: &Agent,
        context: &mut ToolCallContext<'_>,
        call: ToolCallFull,
    ) -> ToolResult {
        let call_id = call.call_id.clone();
        let tool_name = call.name.clone();
        let output = self.call_inner(agent, call, context).await;

        ToolResult::new(tool_name).call_id(call_id).output(output)
    }

    pub async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let mcp_tools = self.mcp_executor.services.list().await?;
        let agent_tools = self.agent_executor.tool_agents().await?;

        let tools = Tools::iter()
            .map(|tool| tool.definition())
            .chain(mcp_tools.into_iter())
            .chain(agent_tools.into_iter())
            .collect::<Vec<_>>();

        Ok(tools)
    }
}

impl<S> ToolRegistry<S> {
    /// Validates if a tool is supported by both the agent and the system.
    ///
    /// # Validation Process
    /// Verifies the tool is supported by the agent specified in the context
    fn validate_tool_call(agent: &Agent, tool_name: &ToolName) -> Result<(), Error> {
        let agent_tools: Vec<_> = agent
            .tools
            .iter()
            .flat_map(|tools| tools.iter())
            .map(|tool| tool.as_str())
            .collect();

        if !agent_tools.contains(&tool_name.as_str())
            && *tool_name != ToolsDiscriminants::ForgeToolAttemptCompletion.name()
        {
            tracing::error!(tool_name = %tool_name, "No tool with name");

            return Err(Error::NotAllowed {
                name: tool_name.clone(),
                supported_tools: agent_tools.join(", "),
            });
        }
        Ok(())
    }

    /// Determines if a tool should have a timeout applied.
    /// According to issue #1402, only search, shell, fetch, and mcp tools
    /// should timeout. Interactive operations (like permission requests)
    /// should not timeout.
    fn should_apply_timeout(tool_name: &ToolName) -> bool {
        let name = tool_name.as_str();
        matches!(
            name,
            "forge_tool_fs_search" |     // search operations
            "forge_tool_process_shell" | // shell operations  
            "forge_tool_net_fetch" /* fetch operations
                                    * MCP tools are handled separately and always have timeouts */
        )
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, ToolName, Tools, ToolsDiscriminants};
    use pretty_assertions::assert_eq;

    use crate::tool_registry::ToolRegistry;

    fn agent() -> Agent {
        // only allow FsRead tool for this agent
        Agent::new(AgentId::new("test_agent")).tools(vec![
            ToolName::new("forge_tool_fs_read"),
            ToolName::new("forge_tool_fs_find"),
        ])
    }

    #[tokio::test]
    async fn test_restricted_tool_call() {
        let result = ToolRegistry::<()>::validate_tool_call(
            &agent(),
            &ToolName::new(Tools::ForgeToolFsRead(Default::default())),
        );
        assert!(result.is_ok(), "Tool call should be valid");
    }

    #[tokio::test]
    async fn test_restricted_tool_call_err() {
        let error = ToolRegistry::<()>::validate_tool_call(
            &agent(),
            &ToolName::new("forge_tool_fs_create"),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            error,
            "Tool 'forge_tool_fs_create' is not available. Please try again with one of these tools: [forge_tool_fs_read, forge_tool_fs_find]"
        );
    }

    #[tokio::test]
    async fn test_completion_tool_call() {
        let result = ToolRegistry::<()>::validate_tool_call(
            &agent(),
            &ToolsDiscriminants::ForgeToolAttemptCompletion.name(),
        );

        assert!(result.is_ok(), "Completion tool call should be valid");
    }

    #[test]
    fn test_should_apply_timeout_for_search_tool() {
        let tool_name = ToolName::new("forge_tool_fs_search");
        assert!(
            ToolRegistry::<()>::should_apply_timeout(&tool_name),
            "Search tools should have timeout applied"
        );
    }

    #[test]
    fn test_should_apply_timeout_for_shell_tool() {
        let tool_name = ToolName::new("forge_tool_process_shell");
        assert!(
            ToolRegistry::<()>::should_apply_timeout(&tool_name),
            "Shell tools should have timeout applied"
        );
    }

    #[test]
    fn test_should_apply_timeout_for_fetch_tool() {
        let tool_name = ToolName::new("forge_tool_net_fetch");
        assert!(
            ToolRegistry::<()>::should_apply_timeout(&tool_name),
            "Fetch tools should have timeout applied"
        );
    }

    #[test]
    fn test_should_not_apply_timeout_for_read_tool() {
        let tool_name = ToolName::new("forge_tool_fs_read");
        assert!(
            !ToolRegistry::<()>::should_apply_timeout(&tool_name),
            "Read tools should not have timeout applied to allow permission prompts"
        );
    }

    #[test]
    fn test_should_not_apply_timeout_for_create_tool() {
        let tool_name = ToolName::new("forge_tool_fs_create");
        assert!(
            !ToolRegistry::<()>::should_apply_timeout(&tool_name),
            "Create tools should not have timeout applied to allow permission prompts"
        );
    }

    #[test]
    fn test_should_not_apply_timeout_for_interactive_tool() {
        let tool_name = ToolName::new("forge_tool_followup");
        assert!(
            !ToolRegistry::<()>::should_apply_timeout(&tool_name),
            "Interactive tools should not have timeout applied"
        );
    }
}
