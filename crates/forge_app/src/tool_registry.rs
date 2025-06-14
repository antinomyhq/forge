use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use forge_domain::{
    Agent, AgentInput, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolOutput,
    ToolResult, Tools,
};
use strum::IntoEnumIterator;
use tokio::time::timeout;

use crate::agent_executor::AgentExecutor;
use crate::error::Error;
use crate::mcp_executor::McpExecutor;
use crate::tool_executor::ToolExecutor;
use crate::{McpService, Services};

const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(300);

pub struct ToolRegistry<S> {
    tool_executor: ToolExecutor<S>,
    agent_executor: AgentExecutor<S>,
    mcp_executor: McpExecutor<S>,
}

impl<S: Services> ToolRegistry<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self {
            tool_executor: ToolExecutor::new(services.clone()),
            agent_executor: AgentExecutor::new(services.clone()),
            mcp_executor: McpExecutor::new(services),
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
        timeout(TOOL_CALL_TIMEOUT, future())
            .await
            .context(Error::CallTimeout {
                timeout: TOOL_CALL_TIMEOUT.as_secs() / 60,
                tool_name: tool_name.clone(),
            })?
    }

    async fn call_inner(
        &self,
        agent: &Agent,
        input: ToolCallFull,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        Self::validate_tool_call(agent, &input.name).await?;

        tracing::info!(tool_name = %input.name, arguments = %input.arguments, "Executing tool call");
        let tool_name = input.name.clone();

        // First, try to call a Forge tool
        if Tools::contains(&input.name) {
            self.call_with_timeout(&tool_name, || {
                self.tool_executor.execute(input.clone(), context)
            })
            .await
        } else if self.agent_executor.contains_tool(&input.name).await? {
            // Handle agent delegation tool calls
            let agent_input: AgentInput =
                serde_json::from_value(input.arguments).context("Failed to parse agent input")?;
            // NOTE: Agents should not timeout
            self.agent_executor
                .execute(input.name.to_string(), agent_input.task, context)
                .await
        } else if self.mcp_executor.contains_tool(&input.name).await? {
            self.call_with_timeout(&tool_name, || self.mcp_executor.execute(input, context))
                .await
        } else {
            Err(Error::NotFound(input.name).into())
        }
    }

    pub async fn call(
        &self,
        agent: &Agent,
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult {
        let call_clone = call.clone();
        let output = self.call_inner(agent, call, context).await;

        ToolResult::new(call_clone.name)
            .call_id(call_clone.call_id)
            .output(output)
    }

    pub async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let mcp_tools = self.mcp_executor.services.mcp_service().list().await?;
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
    async fn validate_tool_call(agent: &Agent, tool_name: &ToolName) -> Result<(), Error> {
        let agent_tools: Vec<_> = agent
            .tools
            .iter()
            .flat_map(|tools| tools.iter())
            .map(|tool| tool.as_str())
            .collect();

        if !agent_tools.contains(&tool_name.as_str()) {
            tracing::error!(tool_name = %tool_name, "No tool with name");

            return Err(Error::NotAllowed {
                name: tool_name.clone(),
                supported_tools: agent_tools.join(", "),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, ToolName, Tools};
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
        )
        .await;
        assert!(result.is_ok(), "Tool call should be valid");
    }

    #[tokio::test]
    async fn test_restricted_tool_call_err() {
        let error = ToolRegistry::<()>::validate_tool_call(
            &agent(),
            &ToolName::new("forge_tool_fs_create"),
        )
        .await
        .unwrap_err()
        .to_string();
        assert_eq!(
            error,
            "Tool 'forge_tool_fs_create' is not available. Please try again with one of these tools: [forge_tool_fs_read, forge_tool_fs_find]"
        );
    }
}
