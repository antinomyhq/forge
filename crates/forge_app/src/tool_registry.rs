use std::cmp::min;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use convert_case::{Case, Casing};
use forge_display::{DiffFormat, GrepFormat, TitleFormat};
use forge_domain::{
    Agent, AgentInput, AttemptCompletion, ChatRequest, ChatResponse, Environment, Event, FSSearch,
    Shell, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolOutput, ToolResult, Tools,
};
use futures::StreamExt;
use regex::Regex;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;
use tokio::time::timeout;

use crate::error::Error;
use crate::utils::{display_path, format_match};
use crate::{
    Content, ConversationService, EnvironmentService, FollowUpService, FsCreateOutput,
    FsCreateService, FsPatchService, FsReadService, FsRemoveService, FsSearchService,
    FsUndoService, HttpResponse, McpService, NetFetchService, PatchOutput, ReadOutput,
    SearchResult, Services, ShellService, WorkflowService,
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

    async fn call_internal(
        &self,
        input: Tools,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<crate::execution_result::ExecutionResult> {
        match input {
            Tools::ForgeToolFsRead(input) => {
                let is_explicit_range = input.start_line.is_some() | input.end_line.is_some();

                let output = self
                    .services
                    .fs_read_service()
                    .read(input.path.clone(), input.start_line, input.end_line)
                    .await?;
                let env = self.services.environment_service().get_environment();
                let display_path = display_path(&env, Path::new(&input.path));
                let is_truncated = output.total_lines > output.end_line;

                send_read_context(
                    context,
                    &output,
                    &display_path,
                    is_explicit_range,
                    is_truncated,
                )
                .await?;

                Ok(crate::execution_result::ExecutionResult::FsRead(output))
            }
            Tools::ForgeToolFsCreate(input) => {
                let out = self
                    .services
                    .fs_create_service()
                    .create(input.path.clone(), input.content, input.overwrite, true)
                    .await?;
                send_write_context(context, &out, &input.path, self.services.as_ref()).await?;

                Ok(crate::execution_result::ExecutionResult::from(out))
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

                send_fs_search_context(self.services.as_ref(), context, &input, &output).await?;

                Ok(crate::execution_result::ExecutionResult::from(output))
            }
            Tools::ForgeToolFsRemove(input) => {
                send_fs_remove_context(context, &input.path, self.services.as_ref()).await?;
                let output = self
                    .services
                    .fs_remove_service()
                    .remove(input.path.clone())
                    .await?;

                Ok(crate::execution_result::ExecutionResult::from(output))
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
                send_fs_patch_context(context, &input.path, &output, self.services.as_ref())
                    .await?;

                Ok(crate::execution_result::ExecutionResult::from(output))
            }
            Tools::ForgeToolFsUndo(input) => {
                send_fs_undo_context(context, input.clone(), self.services.as_ref()).await?;
                let output = self.services.fs_undo_service().undo(input.path).await?;

                Ok(crate::execution_result::ExecutionResult::from(output))
            }
            Tools::ForgeToolProcessShell(input) => {
                send_shell_output_context(
                    context,
                    &input,
                    self.services.environment_service().get_environment(),
                )
                .await?;
                let output = self
                    .services
                    .shell_service()
                    .execute(input.command, input.cwd, input.keep_ansi)
                    .await?;

                Ok(crate::execution_result::ExecutionResult::from(output))
            }
            Tools::ForgeToolNetFetch(input) => {
                let output = self
                    .services
                    .net_fetch_service()
                    .fetch(input.url.clone(), input.raw)
                    .await?;

                send_net_fetch_context(context, &output, &input.url).await?;

                Ok(crate::execution_result::ExecutionResult::from(output))
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

                Ok(crate::execution_result::ExecutionResult::from(output))
            }
            Tools::ForgeToolAttemptCompletion(input) => {
                send_completion_context(context, input).await?;
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

        let out = self.call_internal(tool_input.clone(), context).await?;
        let truncation_path = out.to_create_temp(self.services.as_ref()).await?;
        let env = self.services.environment_service().get_environment();

        Ok(out.into_tool_output(tool_input, truncation_path, &env))
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

async fn send_completion_context(
    ctx: &mut ToolCallContext,
    input: AttemptCompletion,
) -> anyhow::Result<()> {
    ctx.send_summary(input.result).await?;

    Ok(())
}

async fn send_fs_undo_context(
    ctx: &mut ToolCallContext,
    input: forge_domain::FSUndo,
    services: &impl Services,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();

    // Display a message about the file being undone
    let message = TitleFormat::debug("Undo").sub_title(display_path(&env, Path::new(&input.path)));
    ctx.send_text(message).await
}

async fn send_net_fetch_context(
    ctx: &mut ToolCallContext,
    output: &HttpResponse,
    url: &str,
) -> anyhow::Result<()> {
    ctx.send_text(TitleFormat::debug(format!("GET {}", output.code)).sub_title(url))
        .await?;

    Ok(())
}

async fn send_shell_output_context(
    ctx: &mut ToolCallContext,
    output: &Shell,
    environment: Environment,
) -> anyhow::Result<()> {
    let title_format =
        TitleFormat::debug(format!("Execute [{}]", environment.shell)).sub_title(&output.command);
    ctx.send_text(title_format).await?;
    Ok(())
}

async fn send_fs_patch_context<S: Services>(
    ctx: &mut ToolCallContext,
    path: &String,
    output: &PatchOutput,
    services: &S,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();

    let display_path = display_path(&env, Path::new(&path));
    // Generate diff between old and new content
    let diff = DiffFormat::format(&output.before, &output.after);

    ctx.send_text(format!(
        "{}",
        TitleFormat::debug("Patch").sub_title(&display_path)
    ))
    .await?;

    // Output diff either to sender or println
    ctx.send_text(&diff).await?;

    Ok(())
}

async fn send_fs_remove_context<S: Services>(
    ctx: &mut ToolCallContext,
    path: &str,
    service: &S,
) -> anyhow::Result<()> {
    let env = service.environment_service().get_environment();
    let display_path = display_path(&env, Path::new(path));

    let message = TitleFormat::debug("Remove").sub_title(&display_path);

    // Send the formatted message
    ctx.send_text(message).await?;
    Ok(())
}

async fn send_fs_search_context<S: Services>(
    services: &S,
    context: &mut ToolCallContext,
    input: &FSSearch,
    output: &Option<SearchResult>,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();
    let formatted_dir = display_path(&env, Path::new(&input.path));

    let title = match (&input.regex, &input.file_pattern) {
        (Some(regex), Some(pattern)) => {
            format!("Search for '{regex}' in '{pattern}' files at {formatted_dir}")
        }
        (Some(regex), None) => format!("Search for '{regex}' at {formatted_dir}"),
        (None, Some(pattern)) => format!("Search for '{pattern}' at {formatted_dir}"),
        (None, None) => format!("Search at {formatted_dir}"),
    };

    if let Some(output) = output.as_ref() {
        context.send_text(TitleFormat::debug(title)).await?;
        let mut formatted_output = GrepFormat::new(
            output
                .matches
                .iter()
                .map(|v| format_match(v, &env))
                .collect::<Vec<_>>(),
        );
        if let Some(regex) = input.regex.as_ref().and_then(|v| Regex::new(v).ok()) {
            formatted_output = formatted_output.regex(regex);
        }
        context.send_text(formatted_output.format()).await?;
    }

    Ok(())
}

async fn send_write_context<S: Services>(
    ctx: &mut ToolCallContext,
    out: &FsCreateOutput,
    path: &str,
    services: &S,
) -> anyhow::Result<()> {
    let env = services.environment_service().get_environment();
    let formatted_path = display_path(&env, Path::new(&out.path));
    let new_content = services
        .fs_read_service()
        .read(path.to_string(), None, None)
        .await?;
    let exists = out.previous.is_some();

    let title = if exists { "Overwrite" } else { "Create" };

    ctx.send_text(format!(
        "{}",
        TitleFormat::debug(title).sub_title(formatted_path)
    ))
    .await?;

    if let Some(old_content) = out.previous.as_ref() {
        match new_content.content {
            Content::File(new_content) => {
                let diff = DiffFormat::format(old_content, &new_content);
                ctx.send_text(diff).await?;
            }
        }
    }
    Ok(())
}

async fn send_read_context(
    ctx: &mut ToolCallContext,
    out: &ReadOutput,
    display_path: &str,
    is_explicit_range: bool,
    is_truncated: bool,
) -> anyhow::Result<()> {
    let is_range_relevant = is_explicit_range || is_truncated;
    // Set the title based on whether this was an explicit user range request
    // or an automatic limit for large files that actually needed truncation
    let title = if is_explicit_range {
        "Read (Range)"
    } else if is_truncated {
        // Only show "Auto-Limited" if the file was actually truncated
        "Read (Auto-Limited)"
    } else {
        // File was smaller than the limit, so no truncation occurred
        "Read"
    };
    let end_info = min(out.end_line, out.total_lines);
    let range_info = format!(
        "line range: {}-{}, total lines: {}",
        out.start_line, end_info, out.total_lines
    );
    // Build the subtitle conditionally using a string buffer
    let mut subtitle = String::new();

    // Always include the file path
    subtitle.push_str(display_path);

    // Add range info if relevant
    if is_range_relevant {
        // Add range info for explicit ranges or truncated files
        subtitle.push_str(&format!(" [{range_info}]"));
    }
    let message = TitleFormat::debug(title).sub_title(subtitle);
    ctx.send_text(message).await?;
    Ok(())
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
