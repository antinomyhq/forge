use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{bail, Context as AnyhowContext};
use async_recursion::async_recursion;
use chrono::Local;
use forge_walker::Walker;
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
        tool_context: ToolCallContext,
    ) -> anyhow::Result<Vec<ToolCallRecord>> {
        // Always process tool calls sequentially
        let mut tool_call_records = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            // Send the start notification
            self.send(agent, ChatResponse::ToolCallStart(tool_call.clone()))
                .await?;

            // Execute the tool
            let tool_result = self
                .services
                .tool_service()
                .call(tool_context.clone(), tool_call.clone())
                .await;

            // Send the end notification
            self.send(agent, ChatResponse::ToolCallEnd(tool_result.clone()))
                .await?;

            // Add the result to our collection
            tool_call_records.push(ToolCallRecord { tool_call: tool_call.clone(), tool_result });
        }

        Ok(tool_call_records)
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

    /// Get the allowed tools for an agent
    fn get_allowed_tools(&self, agent: &Agent) -> Vec<ToolDefinition> {
        let allowed = agent.tools.iter().flatten().collect::<HashSet<_>>();
        self.services
            .tool_service()
            .list()
            .into_iter()
            .filter(|tool| allowed.contains(&tool.name))
            .collect()
    }

    async fn set_system_prompt(
        &self,
        context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<Context> {
        Ok(if let Some(system_prompt) = &agent.system_prompt {
            let env = self.services.environment_service().get_environment();
            let walker = Walker::max_all().max_depth(agent.max_walker_depth.unwrap_or(1));
            let mut files = walker
                .cwd(env.cwd.clone())
                .get()
                .await?
                .into_iter()
                .map(|f| f.path)
                .collect::<Vec<_>>();
            files.sort();

            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string();

            let tool_information = match agent.tool_supported.unwrap_or_default() {
                true => None,
                false => Some(ToolUsagePrompt::from(&self.get_allowed_tools(agent)).to_string()),
            };

            let ctx = SystemContext {
                current_time,
                env: Some(env),
                tool_information,
                tool_supported: agent.tool_supported.unwrap_or_default(),
                files,
                custom_rules: agent.custom_rules.as_ref().cloned().unwrap_or_default(),
                variables: variables.clone(),
            };

            let system_message = self
                .services
                .template_service()
                .render(system_prompt.template.as_str(), &ctx)?;

            context.set_first_system_message(system_message)
        } else {
            context
        })
    }

    /// Process usage information from a chat completion message
    async fn calculate_usage(
        &self,
        message: &ChatCompletionMessage,
        context: &Context,
        request_usage: Option<Usage>,
        agent: &Agent,
    ) -> anyhow::Result<Option<Usage>> {
        // If usage information is provided by provider use that else depend on
        // estimates.
        let mut usage = message.usage.clone().unwrap_or_default();
        usage.estimated_tokens = Some(context.estimate_token_count());

        debug!(usage = ?usage, "Usage");
        self.send(agent, ChatResponse::Usage(usage.clone())).await?;
        Ok(request_usage.or(Some(usage)))
    }

    async fn collect_messages(
        &self,
        agent: &Agent,
        context: &Context,
        mut response: impl Stream<Item = anyhow::Result<ChatCompletionMessage>> + std::marker::Unpin,
    ) -> anyhow::Result<ChatCompletionResult> {
        let mut messages = Vec::new();
        let mut request_usage: Option<Usage> = None;
        let mut content = String::new();
        let mut xml_tool_calls = None;
        let mut tool_interrupted = false;

        // Only interrupt the loop for XML tool calls if tool_supported is false
        let should_interrupt_for_xml = !agent.tool_supported.unwrap_or_default();

        while let Some(message) = response.next().await {
            let message = message?;
            messages.push(message.clone());

            // Process usage information
            request_usage = self
                .calculate_usage(&message, context, request_usage, agent)
                .await?;

            // Process content
            if let Some(content_part) = message.content.clone() {
                let content_part = content_part.as_str().to_string();

                content.push_str(&content_part);

                // Send partial content to the client
                self.send(
                    agent,
                    ChatResponse::Text {
                        text: content_part,
                        is_complete: false,
                        is_md: false,
                        is_summary: false,
                    },
                )
                .await?;

                // Check for XML tool calls in the content, but only interrupt if tool_supported
                // is false
                if should_interrupt_for_xml {
                    // Use match instead of ? to avoid propagating errors
                    if let Some(tool_call) = ToolCallFull::try_from_xml(&content)
                        .ok()
                        .into_iter()
                        .flatten()
                        .next()
                    {
                        xml_tool_calls = Some(tool_call);
                        tool_interrupted = true;

                        // Break the loop since we found an XML tool call and tool_supported is
                        // false
                        break;
                    }
                }
            }
        }

        // Get the full content from all messages
        let mut content = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .map(|content| content.as_str())
            .collect::<Vec<_>>()
            .join("");

        if tool_interrupted && !content.trim().ends_with("</forge_tool_call>") {
            if let Some((i, right)) = content.rmatch_indices("</forge_tool_call>").next() {
                content.truncate(i + right.len());

                // Add a comment for the assistant to signal interruption
                content.push('\n');
                content.push_str("<forge_feedback>");
                content.push_str(
                    "Response interrupted by tool result. Use only one tool at the end of the message",
                 );
                content.push_str("</forge_feedback>");
            }
        }

        // Send the complete message
        self.send(
            agent,
            ChatResponse::Text {
                text: remove_tag_with_prefix(&content, "forge_")
                    .as_str()
                    .to_string(),
                is_complete: true,
                is_md: true,
                is_summary: false,
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
            .with_context(|| format!("Failed to parse tool call: {tool_call_parts:?}"))?;

        // Combine all sources of tool calls
        let tool_calls: Vec<ToolCallFull> = initial_tool_calls
            .into_iter()
            .chain(partial_tool_calls)
            .chain(xml_tool_calls)
            .collect();

        Ok(ChatCompletionResult { content, tool_calls, usage: request_usage })
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

    // Get the ToolCallContext for an agent
    fn get_tool_call_context(&self, agent_id: &AgentId) -> ToolCallContext {
        // Create a new ToolCallContext with the agent ID
        ToolCallContext::default()
            .agent_id(agent_id.clone())
            .sender(self.sender.clone())
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
            agent.init_context(self.get_allowed_tools(agent)).await?
        } else {
            match conversation.context(&agent.id) {
                Some(context) => context.clone(),
                None => agent.init_context(self.get_allowed_tools(agent)).await?,
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

        let tool_context = self.get_tool_call_context(&agent.id);

        let mut empty_tool_call_count = 0;

        while !tool_context.get_complete().await {
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
                self.collect_messages(agent, &context, response).await?;

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

            let empty_tool_calls = tool_calls.is_empty();

            debug!(
                agent_id = %agent.id,
                tool_call_count = empty_tool_calls,
                "Tool call count: {}",
                empty_tool_calls
            );

            // Process tool calls and update context
            context = context.append_message(
                content,
                self.get_all_tool_results(agent, &tool_calls, tool_context.clone())
                    .await?,
                agent.tool_supported.unwrap_or_default(),
            );

            if empty_tool_calls {
                // No tool calls present, which doesn't mean task is complete so reprompt the
                // agent to ensure the task complete.
                let content = self
                    .services
                    .template_service()
                    .render("{{> partial-tool-required.hbs}}", &())?;
                context = context.add_message(ContextMessage::user(content));

                empty_tool_call_count += 1;

                if empty_tool_call_count > 3 {
                    bail!("Model is unable to follow instructions, consider retrying or switching to a bigger model.");
                }
            }

            // Update context in the conversation
            self.set_context(&agent.id, context.clone()).await?;
            self.sync_conversation().await?;
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
            let event_context = EventContext::new(event.clone()).variables(variables.clone());
            debug!(event_context = ?event_context, "Event context");
            self.services
                .template_service()
                .render(user_prompt.template.as_str(), &event_context)?
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
            .await?;
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
