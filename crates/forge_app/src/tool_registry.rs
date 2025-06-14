use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use convert_case::{Case, Casing};
use forge_display::TitleFormat;
use forge_domain::{
    Agent, AgentInput, ChatRequest, ChatResponse, Event, ToolCallContext, ToolCallFull,
    ToolDefinition, ToolName, ToolOutput, ToolResult, Tools,
};
use futures::StreamExt;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;
use tokio::time::timeout;

use crate::error::Error;
use crate::execution_result::ExecutionResult;
use crate::input_title::InputTitle;
use crate::{
    ConversationService, EnvironmentService, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, McpService, NetFetchService,
    Services, ShellService, WorkflowService,
};

const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(300);

pub struct ToolRegistry<S> {
    services: Arc<S>,
    tool_agents: Arc<RwLock<Option<Vec<ToolDefinition>>>>,
}

impl<S: Services> ToolRegistry<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services, tool_agents: Arc::new(RwLock::new(None)) }
    }

    /// Returns a list of tool definitions for all available agents.
    async fn tool_agents(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        if let Some(tool_agents) = self.tool_agents.read().await.clone() {
            return Ok(tool_agents);
        }
        let workflow = self.services.workflow_service().read_merged(None).await?;

        let agents: Vec<ToolDefinition> = workflow.agents.into_iter().map(Into::into).collect();
        *self.tool_agents.write().await = Some(agents.clone());
        Ok(agents)
    }

    /// Executes an agent tool call by creating a new chat request for the
    /// specified agent.
    async fn call_agent_tool(
        &self,
        agent_id: String,
        task: String,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        context
            .send_text(
                TitleFormat::debug(format!(
                    "{} (Agent)",
                    agent_id.as_str().to_case(Case::UpperSnake)
                ))
                .sub_title(task.as_str()),
            )
            .await?;

        // Create a new conversation for agent execution
        let workflow = self.services.workflow_service().read_merged(None).await?;
        let conversation = self
            .services
            .conversation_service()
            .create(workflow)
            .await?;

        // Execute the request through the ForgeApp
        let app = crate::ForgeApp::new(self.services.clone());
        let mut response_stream = app
            .chat(ChatRequest::new(
                Event::new(format!("{agent_id}/user_task_init"), task),
                conversation.id,
            ))
            .await?;

        // Collect responses from the agent
        while let Some(message) = response_stream.next().await {
            let message = message?;
            match &message {
                ChatResponse::Text { text, is_summary, .. } if *is_summary => {
                    return Ok(ToolOutput::text(text));
                }
                _ => {
                    context.send(message).await?;
                }
            }
        }
        Err(Error::EmptyToolResponse.into())
    }

    async fn call_internal(&self, input: Tools) -> anyhow::Result<ExecutionResult> {
        match input {
            Tools::ForgeToolFsRead(input) => {
                let output = self
                    .services
                    .fs_read_service()
                    .read(input.path.clone(), input.start_line, input.end_line)
                    .await?;

                Ok(output.into())
            }
            Tools::ForgeToolFsCreate(input) => {
                let out = self
                    .services
                    .fs_create_service()
                    .create(input.path.clone(), input.content, input.overwrite, true)
                    .await?;

                Ok((out).into())
            }
            Tools::ForgeToolFsSearch(input) => {
                let output = self
                    .services
                    .fs_search_service()
                    .search(
                        input.path.clone(),
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;

                Ok((output).into())
            }
            Tools::ForgeToolFsRemove(input) => {
                let output = self
                    .services
                    .fs_remove_service()
                    .remove(input.path.clone())
                    .await?;

                Ok((output).into())
            }
            Tools::ForgeToolFsPatch(input) => {
                let output = self
                    .services
                    .fs_patch_service()
                    .patch(
                        input.path.clone(),
                        input.search,
                        input.operation,
                        input.content,
                    )
                    .await?;

                Ok((output).into())
            }
            Tools::ForgeToolFsUndo(input) => {
                let output = self.services.fs_undo_service().undo(input.path).await?;

                Ok((output).into())
            }
            Tools::ForgeToolProcessShell(input) => {
                let output = self
                    .services
                    .shell_service()
                    .execute(input.command, input.cwd, input.keep_ansi)
                    .await?;

                Ok((output).into())
            }
            Tools::ForgeToolNetFetch(input) => {
                let output = self
                    .services
                    .net_fetch_service()
                    .fetch(input.url.clone(), input.raw)
                    .await?;

                Ok((output).into())
            }
            Tools::ForgeToolFollowup(input) => {
                let output = self
                    .services
                    .follow_up_service()
                    .follow_up(
                        input.question,
                        input
                            .option1
                            .into_iter()
                            .chain(input.option2.into_iter())
                            .chain(input.option3.into_iter())
                            .chain(input.option4.into_iter())
                            .chain(input.option5.into_iter())
                            .collect(),
                        input.multiple,
                    )
                    .await?;

                Ok((output).into())
            }
            Tools::ForgeToolAttemptCompletion(_input) => {
                Ok(crate::execution_result::ExecutionResult::AttemptCompletion)
            }
        }
    }
    async fn call_forge_tool(
        &self,
        input: ToolCallFull,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_input = Tools::try_from(input).map_err(Error::CallArgument)?;
        let env = self.services.environment_service().get_environment();
        let title = tool_input.to_title(&env);
        // Send tool call information
        context.send_text(title).await?;
        let execution_result = self.call_internal(tool_input.clone()).await;
        if let Err(ref e) = execution_result {
            // Send failure message
            context.send_text(TitleFormat::error(e.to_string())).await?;
        }
        let execution_result = execution_result?;
        let truncation_path = execution_result
            .to_create_temp(self.services.as_ref())
            .await?;
        Ok(execution_result.into_tool_output(tool_input, truncation_path, &env))
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
        let agent_tools = self.tool_agents().await?;

        tracing::info!(tool_name = %input.name, arguments = %input.arguments, "Executing tool call");
        let tool_name = input.name.clone();

        // First, try to call a Forge tool
        if Tools::contains(&input.name) {
            self.call_with_timeout(&tool_name, || self.call_forge_tool(input.clone(), context))
                .await
        } else if agent_tools.iter().any(|tool| tool.name == input.name) {
            // Handle agent delegation tool calls
            let agent_input: AgentInput =
                serde_json::from_value(input.arguments).context("Failed to parse agent input")?;
            // NOTE: Agents should not timeout
            self.call_agent_tool(input.name.to_string(), agent_input.task, context)
                .await
        } else if self
            .services
            .mcp_service()
            .list()
            .await?
            .iter()
            .any(|tool| tool.name == input.name)
        {
            context
                .send_text(TitleFormat::info("MCP").sub_title(input.name.as_str()))
                .await?;

            self.call_with_timeout(&tool_name, || self.services.mcp_service().call(input))
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
        let mcp_tools = self.services.mcp_service().list().await?;
        let agent_tools = self.tool_agents().await?;

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
