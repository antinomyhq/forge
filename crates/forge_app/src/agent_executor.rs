use std::sync::{Arc, Mutex};

use convert_case::{Case, Casing};
use forge_domain::{
    AgentId, ChatRequest, ChatResponse, ChatResponseContent, Conversation, Event, NoOpHook,
    TitleFormat, ToolCallContext, ToolDefinition, ToolName, ToolOutput,
};
use forge_template::Element;
use futures::StreamExt;
use tokio::sync::RwLock;

use crate::error::Error;
use crate::{AgentRegistry, ConversationService, Services};

#[derive(Clone)]
pub struct AgentExecutor<S> {
    services: Arc<S>,
    pub tool_agents: Arc<RwLock<Option<Vec<ToolDefinition>>>>,
}

impl<S: Services> AgentExecutor<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services, tool_agents: Arc::new(RwLock::new(None)) }
    }

    /// Returns a list of tool definitions for all available agents.
    pub async fn agent_definitions(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        if let Some(tool_agents) = self.tool_agents.read().await.clone() {
            return Ok(tool_agents);
        }
        let agents = self.services.get_agents().await?;
        let tools: Vec<ToolDefinition> = agents.into_iter().map(Into::into).collect();
        *self.tool_agents.write().await = Some(tools.clone());
        Ok(tools)
    }

    /// Executes an agent tool call by creating a new chat request for the
    /// specified agent.
    pub async fn execute(
        &self,
        agent_id: AgentId,
        task: String,
        ctx: &ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        ctx.send_title(
            TitleFormat::debug(format!(
                "{} [Agent]",
                agent_id.as_str().to_case(Case::UpperSnake)
            ))
            .sub_title(task.as_str()),
        )
        .await?;

        // Create a new conversation for agent execution
        let conversation = Conversation::generate().title(task.clone());
        self.services
            .conversation_service()
            .upsert_conversation(conversation.clone())
            .await?;

        // Execute the request through the ForgeApp
        let hook = Arc::new(CodebaseSearchAgentHook::default());
        let mut response_stream = if agent_id.is_codebase_search() {
            let app =
                crate::ForgeApp::<S, NoOpHook>::new(self.services.clone()).with_hook(hook.clone());
            app.chat(
                agent_id.clone(),
                ChatRequest::new(Event::new(task.clone()), conversation.id),
            )
            .await?
        } else {
            let app = crate::ForgeApp::<S, NoOpHook>::new(self.services.clone());
            app.chat(
                agent_id.clone(),
                ChatRequest::new(Event::new(task.clone()), conversation.id),
            )
            .await?
        };

        // Collect responses from the agent
        let mut output = None;
        while let Some(message) = response_stream.next().await {
            let message = message?;
            match message {
                ChatResponse::TaskMessage { ref content } => match content {
                    ChatResponseContent::Title(_) => ctx.send(message).await?,
                    ChatResponseContent::PlainText(text) => output = Some(text.to_owned()),
                    ChatResponseContent::Markdown(text) => output = Some(text.to_owned()),
                },
                ChatResponse::TaskReasoning { .. } => {}
                ChatResponse::TaskComplete => {}
                ChatResponse::ToolCallStart(_) => ctx.send(message).await?,
                ChatResponse::ToolCallEnd(_) => ctx.send(message).await?,
                ChatResponse::RetryAttempt { .. } => ctx.send(message).await?,
                ChatResponse::Interrupt { .. } => ctx.send(message).await?,
            }
        }

        // Prefer the last tool result output, fall back to text output
        if let Some(result) = hook.captured_output.lock().unwrap().take() {
            Ok(ToolOutput::ai(
                conversation.id,
                result.output.as_str().unwrap_or(""),
            ))
        } else if let Some(output) = output {
            Ok(ToolOutput::ai(
                conversation.id,
                Element::new("task_completed")
                    .attr("task", &task)
                    .append(Element::new("output").text(output)),
            ))
        } else {
            Err(Error::EmptyToolResponse.into())
        }
    }

    pub async fn contains_tool(&self, tool_name: &ToolName) -> anyhow::Result<bool> {
        let agent_tools = self.agent_definitions().await?;
        Ok(agent_tools.iter().any(|tool| tool.name == *tool_name))
    }
}

#[derive(Debug, Clone)]
struct CodebaseSearchAgentHook {
    max_iterations: usize,
    captured_output: Arc<Mutex<Option<forge_domain::ToolResult>>>,
}

impl Default for CodebaseSearchAgentHook {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            captured_output: Arc::new(Mutex::new(None)),
        }
    }
}

impl forge_domain::OrchHook for CodebaseSearchAgentHook {
    /// Called before making a call to the provider.
    fn pre_chat<'a>(
        &'a self,
        mut ctx: forge_domain::PreChatContext,
    ) -> forge_domain::BoxFuture<'a, forge_domain::HookResult<forge_domain::ChatAction>> {
        if ctx.iteration + 1 == self.max_iterations {
            ctx.context = ctx
                .context
                .tool_choice(forge_domain::ToolChoice::Call(ToolName::new(
                    "search_report",
                )));
            Box::pin(async move {
                forge_domain::HookResult::Continue(forge_domain::ChatAction::stop(ctx.context))
            })
        } else {
            Box::pin(async move {
                forge_domain::HookResult::Continue(forge_domain::ChatAction::cont(ctx.context))
            })
        }
    }

    /// Called after each tool execution completes.
    fn post_tool_call<'a>(
        &'a self,
        ctx: forge_domain::PostToolCallContext,
    ) -> forge_domain::BoxFuture<'a, forge_domain::HookResult<forge_domain::ToolResult>> {
        *self.captured_output.lock().unwrap() = Some(ctx.result.clone());
        Box::pin(async move { forge_domain::HookResult::Continue(ctx.result) })
    }
}
