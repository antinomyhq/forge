use std::sync::{Arc, Mutex};

use convert_case::{Case, Casing};
use forge_domain::{
    AgentId, ChatRequest, ChatResponse, ChatResponseContent, Conversation,
    ContextMessage, Event, Hook, LifecycleEvent, Role, TextMessage, TitleFormat,
    ToolCallContext, ToolDefinition, ToolName, ToolOutput,
};
use forge_template::Element;
use futures::StreamExt;
use tokio::sync::RwLock;

use crate::error::Error;
use crate::{AgentRegistry, ConversationService, EnvironmentService, Services};

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
        let env = self.services.get_environment();
        let (hook, captured_output) = if agent_id.is_codebase_search() {
            let hook = CodebaseSearchAgentHook::new(env.codebase_search_max_iterations);
            (Some(Arc::new(hook.hook)), hook.captured_output)
        } else {
            (None, Arc::new(Mutex::new(None)))
        };

        let mut response_stream = if let Some(hook) = hook {
            let app = crate::ForgeApp::<S>::new(self.services.clone()).with_hook(hook);
            app.chat(
                agent_id.clone(),
                ChatRequest::new(Event::new(task.clone()), conversation.id),
            )
            .await?
        } else {
            let app = crate::ForgeApp::<S>::new(self.services.clone());
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
        if let Some(result) = captured_output.lock().unwrap().take() {
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

/// Represents a reminder action to be applied to the conversation.
enum ReminderAction {
    /// A soft reminder with a message (no tool forcing).
    Message(String),
    /// A final reminder that forces the agent to call search_report.
    ForceToolCall(String),
}

/// Builds a reminder action based on the current request count and max iterations.
fn build_iteration_reminder(request_count: usize, max_iterations: usize) -> Option<ReminderAction> {
    let remaining = max_iterations.saturating_sub(request_count);
    let halfway = max_iterations / 2;
    let urgent_threshold = max_iterations.saturating_sub(2);

    match request_count {
        0 => None,
        n if n == halfway => Some(ReminderAction::Message(format!(
            "<system-reminder>You have used {n} of {max_iterations} requests. \
             You have {remaining} requests remaining before you must call \
             search_report to report your findings.</system-reminder>"
        ))),
        n if n >= urgent_threshold && n < max_iterations => Some(ReminderAction::Message(format!(
            "<system-reminder>URGENT: You have used {n} of {max_iterations} requests. \
             Only {remaining} request(s) remaining! You MUST call search_report on your \
             next turn to report your findings.</system-reminder>"
        ))),
        n if n == max_iterations + 1 => Some(ReminderAction::ForceToolCall(
            "<system-reminder>FINAL REMINDER: You have reached the maximum number of requests. \
             You MUST call the search_report tool now to report your findings. \
             Do not make any more search requests.</system-reminder>"
                .to_string(),
        )),
        _ => None,
    }
}

/// Applies a reminder action to the conversation context.
fn apply_reminder(conversation: &mut forge_domain::Conversation, action: ReminderAction) {
    let Some(ctx) = conversation.context.take() else {
        return;
    };

    conversation.context = Some(match action {
        ReminderAction::Message(msg) => {
            let message = TextMessage::new(Role::User, msg);
            ctx.add_message(ContextMessage::Text(message))
        }
        ReminderAction::ForceToolCall(msg) => {
            let message = TextMessage::new(Role::User, msg);
            ctx.add_message(ContextMessage::Text(message))
                .tool_choice(forge_domain::ToolChoice::Call(
                    forge_domain::ToolName::new("search_report"),
                ))
        }
    });
}

#[derive(Debug)]
struct CodebaseSearchAgentHook {
    hook: Hook,
    captured_output: Arc<Mutex<Option<forge_domain::ToolResult>>>,
}

impl CodebaseSearchAgentHook {
    fn new(max_iterations: usize) -> Self {
        let captured_output = Arc::new(Mutex::new(None));
        let hook = Hook::default()
            .on_request({
                move |event: LifecycleEvent, conversation: &mut forge_domain::Conversation| {
                    if let LifecycleEvent::Request { request_count, .. } = event {
                        if let Some(action) = build_iteration_reminder(request_count, max_iterations)
                        {
                            apply_reminder(conversation, action);
                        }
                    }
                    async move { Ok(()) }
                }
            })
            .on_toolcall_end({
                let captured_output = captured_output.clone();
                move |event: LifecycleEvent, _conversation: &mut forge_domain::Conversation| {
                    let captured_output = captured_output.clone();
                    async move {
                        if let LifecycleEvent::ToolcallEnd(result) = event {
                            *captured_output.lock().unwrap() = Some(result);
                        }
                        Ok(())
                    }
                }
            });

        Self { hook, captured_output }
    }
}
