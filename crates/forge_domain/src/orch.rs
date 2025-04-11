use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context as AnyhowContext;
use async_recursion::async_recursion;
use futures::future::join_all;
use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio::sync::RwLock;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;
use tracing::debug;

// Use retry_config default values directly in this file
use crate::compaction::ContextCompactor;
use crate::services::Services;
use crate::*;

type ArcSender = Arc<tokio::sync::mpsc::Sender<anyhow::Result<AgentMessage<ChatResponse>>>>;

#[derive(Debug, Clone)]
pub struct AgentMessage<T> {
    pub agent: AgentId,
    pub message: T,
}

#[derive(Clone)]
pub struct Orchestrator<App> {
    services: Arc<App>,
    sender: Option<ArcSender>,
    conversation: Arc<RwLock<Conversation>>,
    compactor: ContextCompactor<App>,
    retry_strategy: std::iter::Take<tokio_retry::strategy::ExponentialBackoff>,
}

struct ChatCompletionResult {
    pub content: String,
    pub tool_calls: Vec<ToolCallFull>,
    pub usage: Option<Usage>,
}

impl<A: Services> Orchestrator<A> {
    pub fn new(
        services: Arc<A>,
        mut conversation: Conversation,
        sender: Option<ArcSender>,
    ) -> Self {
        // since self is a new request, we clear the queue
        conversation.state.values_mut().for_each(|state| {
            state.queue.clear();
        });

        let env = services.environment_service().get_environment();
        let retry_strategy = ExponentialBackoff::from_millis(env.retry_config.initial_backoff_ms)
            .factor(env.retry_config.backoff_factor)
            .take(env.retry_config.max_retry_attempts);

        Self {
            compactor: ContextCompactor::new(services.clone()),
            services,
            sender,
            retry_strategy,
            conversation: Arc::new(RwLock::new(conversation)),
        }
    }

    // Helper function to get all tool results from a vector of tool calls
    #[async_recursion]
    async fn get_all_tool_results(
        &self,
        agent: &Agent,
        tool_calls: &[ToolCallFull],
    ) -> anyhow::Result<Vec<ToolResult>> {
        let mut tool_results = Vec::new();

        for tool_call in tool_calls.iter() {
            self.send(agent, ChatResponse::ToolCallStart(tool_call.clone()))
                .await?;
            let tool_result = self.execute_tool(agent, tool_call).await?;
            tool_results.push(tool_result.clone());
            self.send(agent, ChatResponse::ToolCallEnd(tool_result))
                .await?;
        }

        Ok(tool_results)
    }

    async fn send(&self, agent: &Agent, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            // Send message if it's a Custom type or if hide_content is false
            let show_text = !agent.hide_content.unwrap_or_default();
            let can_send = !matches!(&message, ChatResponse::Text(_)) || show_text;
            if can_send {
                sender
                    .send(Ok(AgentMessage { agent: agent.id.clone(), message }))
                    .await?
            }
        }
        Ok(())
    }

    fn init_default_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.services.tool_service().list()
    }

    fn init_tool_definitions(&self, agent: &Agent) -> Vec<ToolDefinition> {
        let allowed = agent.tools.iter().flatten().collect::<HashSet<_>>();
        let mut forge_tools = self.init_default_tool_definitions();

        // Adding Event tool to the list of tool definitions
        forge_tools.push(Event::tool_definition());

        forge_tools
            .into_iter()
            .filter(|tool| allowed.contains(&tool.name))
            .collect::<Vec<_>>()
    }

    async fn init_agent_context(&self, agent: &Agent) -> anyhow::Result<Context> {
        let mut tool_defs = self.init_tool_definitions(agent);
        self.services.mcp_service().init_mcp().await?;
        let mcp_tools = self.services.mcp_service().list_tools().await?;
        tool_defs.extend(mcp_tools);

        // Use the agent's tool_supported flag directly instead of querying the provider
        let tool_supported = agent.tool_supported.unwrap_or_default();

        let context = Context::default();

        Ok(context.extend_tools(if tool_supported {
            tool_defs
        } else {
            Vec::new()
        }))
    }

    async fn set_system_prompt(
        &self,
        context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<Context> {
        Ok(if let Some(system_prompt) = &agent.system_prompt {
            let system_message = self
                .services
                .template_service()
                .render_system(agent, system_prompt, variables)
                .await?;

            context.set_first_system_message(system_message)
        } else {
            context
        })
    }

    async fn collect_messages(
        &self,
        agent: &Agent,
        mut response: impl Stream<Item = std::result::Result<ChatCompletionMessage, anyhow::Error>>
            + std::marker::Unpin,
    ) -> anyhow::Result<ChatCompletionResult> {
        let mut messages = Vec::new();
        let mut request_usage: Option<Usage> = None;

        while let Some(message) = response.next().await {
            let message = message?;
            messages.push(message.clone());
            if let Some(content) = message.content {
                self.send(agent, ChatResponse::Text(content.as_str().to_string()))
                    .await?;
            }

            if let Some(usage) = message.usage {
                request_usage = Some(usage.clone());
                debug!(usage = ?usage, "Usage");
                self.send(agent, ChatResponse::Usage(usage)).await?;
            }
        }

        let content = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .map(|content| content.as_str())
            .collect::<Vec<_>>()
            .join("");

        // From Complete (incase streaming is disabled)
        let mut tool_calls: Vec<ToolCallFull> = messages
            .iter()
            .flat_map(|message| message.tool_call.iter())
            .filter_map(|message| message.as_full().cloned())
            .collect::<Vec<_>>();

        // From partial tool calls
        let tool_call_parts = messages
            .iter()
            .filter_map(|message| message.tool_call.first())
            .clone()
            .filter_map(|tool_call| tool_call.as_partial().cloned())
            .collect::<Vec<_>>();

        tool_calls.extend(
            ToolCallFull::try_from_parts(&tool_call_parts)
                .with_context(|| format!("Failed to parse tool call: {:?}", tool_call_parts))?,
        );

        // From XML
        tool_calls.extend(ToolCallFull::try_from_xml(&content)?);

        Ok(ChatCompletionResult { content, tool_calls, usage: request_usage })
    }

    pub async fn dispatch_spawned(&self, event: Event) -> anyhow::Result<()> {
        let this = self.clone();
        let _ = tokio::spawn(async move { this.dispatch(event).await }).await?;
        Ok(())
    }

    pub async fn dispatch(&self, event: Event) -> anyhow::Result<()> {
        let inactive_agents = {
            let mut conversation = self.conversation.write().await;
            debug!(
                conversation_id = %conversation.id,
                event_name = %event.name,
                event_value = %event.value,
                "Dispatching event"
            );
            conversation.dispatch_event(event)
        };

        // Execute all initialization futures in parallel
        join_all(inactive_agents.iter().map(|id| self.wake_agent(id)))
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<()>>>()?;

        Ok(())
    }

    #[async_recursion]
    async fn execute_tool(
        &self,
        agent: &Agent,
        tool_call: &ToolCallFull,
    ) -> anyhow::Result<ToolResult> {
        if let Some(event) = Event::parse(tool_call) {
            self.send(agent, ChatResponse::Event(event.clone())).await?;

            self.dispatch_spawned(event).await?;
            Ok(ToolResult::from(tool_call.clone()).success("Event Dispatched Successfully"))
        } else if self
            .services
            .tool_service()
            .list()
            .iter()
            .any(|v| tool_call.name.eq(&v.name))
        {
            Ok(self.services.tool_service().call(tool_call.clone()).await)
        } else {
            self.services.mcp_service().init_mcp().await?;
            let result = self
                .services
                .mcp_service()
                .call_tool(tool_call.name.as_str(), tool_call.arguments.clone())
                .await?;
            let _ = self.services.mcp_service().stop_all_servers().await;
            Ok(ToolResult {
                name: tool_call.name.clone(),
                call_id: tool_call.call_id.clone(),
                content: serde_json::to_string(&result.content)?,
                is_error: result.is_error.unwrap_or_default(),
            })
        }
    }

    async fn sync_conversation(&self) -> anyhow::Result<()> {
        let conversation = self.conversation.read().await.clone();
        self.services
            .conversation_service()
            .upsert(conversation)
            .await?;
        Ok(())
    }

    async fn get_conversation(&self) -> anyhow::Result<Conversation> {
        Ok(self.conversation.read().await.clone())
    }

    async fn complete_turn(&self, agent_id: &AgentId) -> anyhow::Result<()> {
        let mut conversation = self.conversation.write().await;
        conversation
            .state
            .entry(agent_id.clone())
            .or_default()
            .turn_count += 1;
        Ok(())
    }

    async fn set_context(&self, agent_id: &AgentId, context: Context) -> anyhow::Result<()> {
        let mut conversation = self.conversation.write().await;
        conversation
            .state
            .entry(agent_id.clone())
            .or_default()
            .context = Some(context);
        Ok(())
    }

    // Create a helper method with the core functionality
    async fn init_agent(&self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
        let conversation = self.get_conversation().await?;
        let variables = &conversation.variables;
        debug!(
            conversation_id = %conversation.id,
            agent = %agent_id,
            event = ?event,
            "Initializing agent"
        );
        let agent = conversation.get_agent(agent_id)?;

        let mut context = if agent.ephemeral.unwrap_or_default() {
            self.init_agent_context(agent).await?
        } else {
            match conversation.context(&agent.id) {
                Some(context) => context.clone(),
                None => self.init_agent_context(agent).await?,
            }
        };

        // Render the system prompts with the variables
        context = self.set_system_prompt(context, agent, variables).await?;

        // Render user prompts
        context = self
            .set_user_prompt(context, agent, variables, event)
            .await?;

        if let Some(temperature) = agent.temperature {
            context = context.temperature(temperature);
        }

        // Process attachments
        let attachments = self
            .services
            .attachment_service()
            .attachments(&event.value.to_string())
            .await?;

        for attachment in attachments.into_iter() {
            match attachment.content_type {
                ContentType::Image => {
                    context = context.add_message(ContextMessage::Image(attachment.content));
                }
                ContentType::Text => {
                    let content = format!(
                        "<file_content path=\"{}\">{}</file_content>",
                        attachment.path, attachment.content
                    );
                    context = context.add_message(ContextMessage::user(content));
                }
            }
        }

        self.set_context(&agent.id, context.clone()).await?;

        loop {
            // Set context for the current loop iteration
            self.set_context(&agent.id, context.clone()).await?;

            // Determine which model to use - prefer workflow model if available, fallback
            // to agent model
            let model_id = agent
                .model
                .as_ref()
                .ok_or(Error::MissingModel(agent.id.clone()))?;

            let response = self
                .services
                .provider_service()
                .chat(model_id, context.clone())
                .await?;
            let ChatCompletionResult { tool_calls, content, usage } =
                self.collect_messages(agent, response).await?;
            // Check if context requires compression
            // Only pass prompt_tokens for compaction decision
            context = self
                .compactor
                .compact_context(
                    agent,
                    context,
                    usage.map(|usage| usage.prompt_tokens as usize),
                )
                .await?;

            // Get all tool results using the helper function
            let tool_results = self.get_all_tool_results(agent, &tool_calls).await?;

            context = context
                .add_message(ContextMessage::assistant(content, Some(tool_calls)))
                .add_tool_results(tool_results.clone());

            self.set_context(&agent.id, context.clone()).await?;
            self.sync_conversation().await?;

            if tool_results.is_empty() {
                break;
            }
        }

        self.complete_turn(&agent.id).await?;

        self.sync_conversation().await?;

        Ok(())
    }

    async fn set_user_prompt(
        &self,
        mut context: Context,

        agent: &Agent,
        variables: &HashMap<String, Value>,
        event: &Event,
    ) -> anyhow::Result<Context> {
        let content = if let Some(user_prompt) = &agent.user_prompt {
            // Use the consolidated render_event method which handles suggestions and
            // variables
            self.services
                .template_service()
                .render_event(agent, user_prompt, event, variables)
                .await?
        } else {
            // Use the raw event value as content if no user_prompt is provided
            event.value.to_string()
        };

        if !content.is_empty() {
            context = context.add_message(ContextMessage::user(content));
        }

        Ok(context)
    }

    async fn wake_agent(&self, agent_id: &AgentId) -> anyhow::Result<()> {
        while let Some(event) = {
            let mut conversation = self.conversation.write().await;
            conversation.poll_event(agent_id)
        } {
            RetryIf::spawn(
                self.retry_strategy.clone().map(jitter),
                || self.init_agent(agent_id, &event),
                is_parse_error,
            )
            .await
            .with_context(|| format!("Failed to initialize agent {}", *agent_id))?;
        }

        Ok(())
    }
}

fn is_parse_error(error: &anyhow::Error) -> bool {
    let check = error
        .downcast_ref::<Error>()
        .map(|error| {
            matches!(
                error,
                Error::ToolCallParse(_) | Error::ToolCallArgument(_) | Error::ToolCallMissingName
            )
        })
        .unwrap_or_default();

    if check {
        debug!(error = %error, "Retrying due to parse error");
    }

    check
}
