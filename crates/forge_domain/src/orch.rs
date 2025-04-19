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
use crate::services::Services;
use crate::*;

type ArcSender = Arc<tokio::sync::mpsc::Sender<anyhow::Result<AgentMessage<ChatResponse>>>>;
// Tags to filter as defined in system prompt
const THINKING_TAGS: &[&str] = &[
    "thinking",
    "analysis",
    "action_plan",
    "execution",
    "verification",
    "forge_analysis",
    "forge_query_analysis",
    "pr_preparation",
    "thought_process",
    "exploration_and_discovery",
    "content_plan",
    "creation",
    "review",
    "tool_call",
];

#[derive(Debug, Clone)]
pub struct AgentMessage<T> {
    pub agent: AgentId,
    pub message: T,
}

impl<T> AgentMessage<T> {
    pub fn new(agent: AgentId, message: T) -> Self {
        Self { agent, message }
    }
}

#[derive(Clone)]
pub struct Orchestrator<Services> {
    services: Arc<Services>,
    sender: Option<ArcSender>,
    conversation: Arc<RwLock<Conversation>>,
    retry_strategy: std::iter::Take<tokio_retry::strategy::ExponentialBackoff>,
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
            services,
            sender,
            retry_strategy,
            conversation: Arc::new(RwLock::new(conversation)),
        }
    }

    // Helper method to handle tool-unsupported cases by converting tool calls to
    // content
    async fn handle_unsupported_tools(
        &self,
        agent: &Agent,
        full_tool_calls: &[ToolCallFull],
        content: String,
        mut context: Context,
    ) -> anyhow::Result<Context> {
        // If tools are not supported, get tool results and convert to content instead
        debug!(agent_id = %agent.id, "Tools not supported, adding tool results as separate messages");

        // Get all tool results
        let tool_results = self.get_all_tool_results(agent, full_tool_calls).await?;

        // Add the original assistant message without tool calls
        context = context.add_message(ContextMessage::assistant(content.as_str(), None));

        // Add each tool result as a separate user message to make them distinct
        for result in &tool_results {
            // Add as a separate system message to distinguish it from user and assistant
            // messages
            context = context.add_message(ContextMessage::user(result.to_string()));
        }

        // Return updated context and signal to break the loop
        Ok(context)
    }

    // Helper method to handle standard tool-supported flow
    async fn handle_supported_tools(
        &self,
        agent: &Agent,
        full_tool_calls: &[ToolCallFull],
        content_str: String,
        mut context: Context,
    ) -> anyhow::Result<Context> {
        // Get all tool results using the helper function
        let records = self.get_all_tool_results(agent, full_tool_calls).await?;

        context = context
            .add_message(ContextMessage::assistant(
                content_str,
                Some(full_tool_calls.to_vec()),
            ))
            .add_tool_results(
                records
                    .into_iter()
                    .map(|record| record.tool_result)
                    .collect::<Vec<_>>(),
            );

        // Update context after modifications
        self.set_context(&agent.id, context.clone()).await?;
        self.sync_conversation().await?;

        // Return updated context and signal whether to break loop
        Ok(context)
    }

    // Helper method to process tool calls based on agent capabilities
    async fn process_tool_calls(
        &self,
        agent: &Agent,
        tool_calls: Vec<ToolCall>,
        content: Option<Content>,
        context: Context,
    ) -> anyhow::Result<Context> {
        // We have Vec<ToolCall> but need to extract Vec<ToolCallFull> for tool
        // processing
        let full_tool_calls: Vec<ToolCallFull> = tool_calls
            .iter()
            .filter_map(|tc| tc.as_full().cloned())
            .collect();

        // For usage in ContextMessage::assistant
        let content = content.map_or(String::new(), |c| c.as_str().to_string());

        // Check if tools are supported for this agent
        if !agent.tool_supported.unwrap_or_default() && !full_tool_calls.is_empty() {
            self.handle_unsupported_tools(agent, &full_tool_calls, content, context)
                .await
        } else {
            self.handle_supported_tools(agent, &full_tool_calls, content, context)
                .await
        }
    }

    // Helper function to get all tool results from a vector of tool calls
    #[async_recursion]
    async fn get_all_tool_results(
        &self,
        agent: &Agent,
        tool_calls: &[ToolCallFull],
    ) -> anyhow::Result<Vec<CallRecord>> {
        // Always process tool calls sequentially
        let mut tool_results = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            // Send the start notification
            self.send(agent, ChatResponse::ToolCallStart(tool_call.clone()))
                .await?;

            // Execute the tool
            let tool_result = self.execute_tool(agent, tool_call).await?;

            // Send the end notification
            self.send(agent, ChatResponse::ToolCallEnd(tool_result.clone()))
                .await?;

            // Add the result to our collection
            tool_results.push(CallRecord { tool_call: tool_call.clone(), tool_result });
        }

        Ok(tool_results)
    }

    async fn send(&self, agent: &Agent, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            // Send message if it's a Custom type or if hide_content is false
            let show_text = !agent.hide_content.unwrap_or_default();
            let can_send = !matches!(&message, ChatResponse::Text { .. }) || show_text;
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
        let tool_defs = self.init_tool_definitions(agent);

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
    ) -> anyhow::Result<ChatCompletionMessage> {
        let mut messages = Vec::new();
        let mut request_usage: Option<Usage> = None;

        while let Some(message) = response.next().await {
            let message = message?;
            messages.push(message.clone());
            if let Some(content) = message.content {
                self.send(
                    agent,
                    ChatResponse::Text {
                        text: content.as_str().to_string(),
                        is_complete: false,
                        is_md: false,
                    },
                )
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

        self.send(
            agent,
            ChatResponse::Text {
                text: remove_tag_content(&content, THINKING_TAGS)
                    .as_str()
                    .to_string(),
                is_complete: true,
                is_md: true,
            },
        )
        .await?;

        // Extract all tool calls in a fully declarative way with combined sources
        // Start with complete tool calls (for non-streaming mode)
        let initial_tool_calls: Vec<ToolCallFull> = messages
            .iter()
            .flat_map(|message| &message.tool_calls)
            .filter_map(|tool_call| tool_call.as_full().cloned())
            .collect();

        // Get partial tool calls
        let tool_call_parts: Vec<ToolCallPart> = messages
            .iter()
            .flat_map(|message| &message.tool_calls)
            .filter_map(|tool_call| tool_call.as_partial().cloned())
            .collect();

        // Process partial tool calls
        let partial_tool_calls = ToolCallFull::try_from_parts(&tool_call_parts)
            .with_context(|| format!("Failed to parse tool call: {:?}", tool_call_parts))?;

        // Process XML tool calls
        let xml_tool_calls = ToolCallFull::try_from_xml(&content)?;

        // Combine all sources of tool calls
        let tool_calls: Vec<ToolCallFull> = initial_tool_calls
            .into_iter()
            .chain(partial_tool_calls)
            .chain(xml_tool_calls)
            .collect();

        // Convert Vec<ToolCallFull> to Vec<ToolCall> for use with ChatCompletionMessage
        let tool_calls_vec: Vec<ToolCall> = tool_calls
            .iter()
            .map(|tc| ToolCall::Full(tc.clone()))
            .collect();

        Ok(ChatCompletionMessage::default()
            .content(Content::full(content))
            .tool_calls(tool_calls_vec)
            .usage(request_usage.unwrap_or_default()))
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

    async fn execute_tool(
        &self,
        agent: &Agent,
        tool_call: &ToolCallFull,
    ) -> anyhow::Result<ToolResult> {
        if let Some(event) = Event::parse(tool_call) {
            self.send(agent, ChatResponse::Event(event.clone())).await?;

            self.dispatch_spawned(event).await?;
            Ok(ToolResult::from(tool_call.clone()).success("Event Dispatched Successfully"))
        } else {
            Ok(self
                .services
                .tool_service()
                .call(
                    ToolCallContext::default()
                        .sender(self.sender.clone())
                        .agent_id(agent.id.clone()),
                    tool_call.clone(),
                )
                .await)
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

        // Process attachments in a more declarative way
        let attachments = self
            .services
            .attachment_service()
            .attachments(&event.value.to_string())
            .await?;

        // Process each attachment and fold the results into the context
        context = attachments
            .into_iter()
            .fold(context.clone(), |ctx, attachment| {
                match attachment.content_type {
                    ContentType::Image => {
                        ctx.add_message(ContextMessage::Image(attachment.content))
                    }
                    ContentType::Text => {
                        let content = format!(
                            "<file_content path=\"{}\">{}</file_content>",
                            attachment.path, attachment.content
                        );
                        ctx.add_message(ContextMessage::user(content))
                    }
                }
            });

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

            let ChatCompletionMessage { tool_calls, content, usage, finish_reason: _ } =
                self.collect_messages(agent, response).await?;

            // Check if context requires compression and decide to compact
            if agent.should_compact(&context, usage.map(|usage| usage.prompt_tokens as usize)) {
                debug!(agent_id = %agent.id, "Compaction needed, applying compaction");
                context = self
                    .services
                    .compaction_service()
                    .compact_context(agent, context)
                    .await?;
            } else {
                debug!(agent_id = %agent.id, "Compaction not needed");
            }

            let tool_call_count = tool_calls.is_empty();

            debug!(
                agent_id = %agent.id,
                tool_call_count = tool_call_count,
                "Tool call count: {}",
                tool_call_count
            );

            // Process tool calls and update context
            context = self
                .process_tool_calls(agent, tool_calls, content, context)
                .await?;

            // Update context in the conversation
            self.set_context(&agent.id, context.clone()).await?;
            self.sync_conversation().await?;

            if tool_call_count {
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
