use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context as AnyhowContext;
use async_recursion::async_recursion;
use futures::future::join_all;
use futures::{Stream, StreamExt};
use tracing::debug;

use crate::summarize::token_count;
use crate::*;

type ArcSender = Arc<tokio::sync::mpsc::Sender<anyhow::Result<AgentMessage<ChatResponse>>>>;

#[derive(Debug, Clone)]
pub struct AgentMessage<T> {
    pub agent: AgentId,
    pub message: T,
}

#[derive(Clone)]
pub struct Orchestrator<App> {
    app: Arc<App>,
    sender: Option<ArcSender>,
    conversation_id: ConversationId,
}

struct ChatCompletionResult {
    pub content: String,
    pub tool_calls: Vec<ToolCallFull>,
}

impl<A: App> Orchestrator<A> {
    pub fn new(svc: Arc<A>, conversation_id: ConversationId, sender: Option<ArcSender>) -> Self {
        Self { app: svc, sender, conversation_id }
    }

    async fn send_message(&self, agent_id: &AgentId, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender
                .send(Ok(AgentMessage { agent: agent_id.clone(), message }))
                .await?
        }
        Ok(())
    }

    async fn send(&self, agent_id: &AgentId, message: ChatResponse) -> anyhow::Result<()> {
        self.send_message(agent_id, message).await
    }

    fn init_default_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.app.tool_service().list()
    }

    fn init_tool_definitions(&self, agent: &Agent) -> Vec<ToolDefinition> {
        let allowed = agent.tools.iter().flatten().collect::<HashSet<_>>();
        let mut forge_tools = self.init_default_tool_definitions();

        // Adding self to the list of tool definitions

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

        let mut context = Context::default();

        if let Some(system_prompt) = &agent.system_prompt {
            let system_message = self
                .app
                .template_service()
                .render_system(agent, system_prompt)
                .await?;

            context = context.set_first_system_message(system_message);
        }

        Ok(context.extend_tools(if tool_supported {
            tool_defs
        } else {
            Vec::new()
        }))
    }

    async fn collect_messages(
        &self,
        agent: &AgentId,
        mut response: impl Stream<Item = std::result::Result<ChatCompletionMessage, anyhow::Error>>
            + std::marker::Unpin,
    ) -> anyhow::Result<ChatCompletionResult> {
        let mut messages = Vec::new();

        while let Some(message) = response.next().await {
            let message = message?;
            messages.push(message.clone());
            if let Some(content) = message.content {
                self.send(agent, ChatResponse::Text(content.as_str().to_string()))
                    .await?;
            }

            if let Some(usage) = message.usage {
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

        Ok(ChatCompletionResult { content, tool_calls })
    }

    pub async fn dispatch_spawned(&self, event: Event) -> anyhow::Result<()> {
        let this = self.clone();
        let _ = tokio::spawn(async move { this.dispatch(event).await }).await?;
        Ok(())
    }

    pub async fn dispatch(&self, event: Event) -> anyhow::Result<()> {
        debug!(
            conversation_id = %self.conversation_id,
            event_name = %event.name,
            event_value = %event.value,
            "Dispatching event"
        );

        let agents = self
            .app
            .conversation_service()
            .update(&self.conversation_id, |conversation| {
                conversation.dispatch_event(event)
            })
            .await?;

        // Execute all initialization futures in parallel
        join_all(agents.iter().map(|id| self.init_agent(id)))
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<()>>>()?;

        Ok(())
    }

    #[async_recursion]
    async fn execute_tool(
        &self,
        agent_id: &AgentId,
        tool_call: &ToolCallFull,
    ) -> anyhow::Result<Option<ToolResult>> {
        if let Some(event) = Event::parse(tool_call) {
            self.send(agent_id, ChatResponse::Custom(event.clone()))
                .await?;

            self.dispatch_spawned(event).await?;
            Ok(Some(
                ToolResult::from(tool_call.clone()).success("Event Dispatched Successfully"),
            ))
        } else {
            Ok(Some(self.app.tool_service().call(tool_call.clone()).await))
        }
    }

    #[async_recursion]
    async fn execute_transform(
        &self,
        transforms: &[Transform],
        mut context: Context,
    ) -> anyhow::Result<Context> {
        for transform in transforms.iter() {
            match transform {
                Transform::Assistant {
                    agent_id,
                    token_limit,
                    input: input_key,
                    output: output_key,
                } => {
                    let mut summarize = Summarize::new(&mut context, *token_limit);
                    while let Some(mut summary) = summarize.summarize() {
                        let input = Event::new(input_key, summary.get());
                        self.init_agent_with_event(agent_id, &input).await?;

                        if let Some(value) = self.get_last_event(output_key).await? {
                            summary.set(serde_json::to_string(&value)?);
                        }
                    }
                }
                Transform::User { agent_id, input: input_key, output: output_key } => {
                    if let Some(ContextMessage::ContentMessage(ContentMessage {
                        role: Role::User,
                        content,
                        ..
                    })) = context.messages.last_mut()
                    {
                        let task = Event::new(input_key, content.clone());
                        self.init_agent_with_event(agent_id, &task).await?;

                        if let Some(output) = self.get_last_event(output_key).await? {
                            let message = &output.value;
                            content
                                .push_str(&format!("\n<{output_key}>\n{message}\n</{output_key}>"));
                        }
                        debug!(content = %content, "Transforming user input");
                    }
                }
                Transform::PassThrough { agent_id, input: input_key } => {
                    let input = Event::new(input_key, context.to_text());

                    // NOTE: Tap transformers will not modify the context
                    self.init_agent_with_event(agent_id, &input).await?;
                }
                Transform::AutoCompact { token_limit } => {
                    let total_tokens = token_count(&context.to_text());

                    if total_tokens > *token_limit {
                        let mut summarize = Summarize::new(&mut context, *token_limit);

                        // Apply summarization until we're below the threshold
                        while let Some(mut summary) = summarize.summarize() {
                            let turn_content = summary.get();
                            let summary_text = format!(
                                "Auto-summarized conversation turn with {} tokens",
                                token_count(&turn_content)
                            );
                            summary.set(summary_text);
                        }

                        debug!(
                            conversation_id = %self.conversation_id,
                            tokens_before = %total_tokens,
                            threshold = %token_limit,
                            tokens_after = %token_count(&context.to_text()),
                            "Auto-compaction performed"
                        );
                    }
                }
            }
        }

        Ok(context)
    }

    async fn get_last_event(&self, name: &str) -> anyhow::Result<Option<Event>> {
        Ok(self.get_conversation().await?.rfind_event(name).cloned())
    }

    async fn get_conversation(&self) -> anyhow::Result<Conversation> {
        Ok(self
            .app
            .conversation_service()
            .get(&self.conversation_id)
            .await?
            .ok_or(Error::ConversationNotFound(self.conversation_id.clone()))?)
    }

    async fn complete_turn(&self, agent: &AgentId) -> anyhow::Result<()> {
        self.app
            .conversation_service()
            .inc_turn(&self.conversation_id, agent)
            .await
    }

    async fn set_context(&self, agent: &AgentId, context: Context) -> anyhow::Result<()> {
        self.app
            .conversation_service()
            .set_context(&self.conversation_id, agent, context)
            .await
    }

    async fn init_agent_with_event(&self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
        debug!(
            conversation_id = %self.conversation_id,
            agent = %agent_id,
            event = ?event,
            "Initializing agent"
        );
        let conversation = self.get_conversation().await?;
        let agent = conversation.workflow.get_agent(agent_id)?;

        let mut context = if agent.ephemeral.unwrap_or_default() {
            self.init_agent_context(agent).await?
        } else {
            match conversation.context(&agent.id) {
                Some(context) => context.clone(),
                None => self.init_agent_context(agent).await?,
            }
        };

        let content = if let Some(user_prompt) = &agent.user_prompt {
            // Get conversation variables from the conversation
            let variables = &conversation.variables;

            // Use the consolidated render_event method which handles suggestions and
            // variables
            self.app
                .template_service()
                .render_event(agent, user_prompt, event, variables)
                .await?
        } else {
            // Use the raw event value as content if no user_prompt is provided
            event.value.clone()
        };

        if !content.is_empty() {
            context = context.add_message(ContextMessage::user(content));
        }

        // Process attachments
        let attachments = self
            .app
            .attachment_service()
            .attachments(&event.value)
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

        // Check if auto_compact is enabled for this agent and perform summarization if needed
        if let Some(auto_compact_threshold) = agent.auto_compact {
            let total_tokens = token_count(&context.to_text()) as u64;

            if total_tokens > auto_compact_threshold {
                // Create a summarizer with the auto_compact threshold as token limit
                let mut summarize = Summarize::new(&mut context, auto_compact_threshold as usize);

                // Apply summarization until we're below the threshold
                while let Some(mut summary) = summarize.summarize() {
                    let turn_content = summary.get();
                    // Create a simple summary of the turn
                    let summary_text = format!(
                        "Auto-summarized conversation turn with {} tokens",
                        token_count(&turn_content)
                    );
                    summary.set(summary_text);
                }

                // Update the context in the conversation with the summarized version
                self.set_context(&agent.id, context.clone()).await?;

                // Log that auto-compaction was performed
                debug!(
                    conversation_id = %self.conversation_id,
                    agent = %agent_id,
                    tokens_before = %total_tokens,
                    threshold = %auto_compact_threshold,
                    tokens_after = %(token_count(&context.to_text()) as u64),
                    "Auto-compaction performed"
                );
            }
        }

        loop {
            context = self
                .execute_transform(
                    agent.transforms.as_ref().map_or(&[], |t| t.as_slice()),
                    context,
                )
                .await?;
            self.set_context(&agent.id, context.clone()).await?;
            let response = self
                .app
                .provider_service()
                .chat(
                    agent
                        .model
                        .as_ref()
                        .ok_or(Error::MissingModel(agent.id.clone()))?,
                    context.clone(),
                )
                .await?;
            let ChatCompletionResult { tool_calls, content } =
                self.collect_messages(&agent.id, response).await?;

            let mut tool_results = Vec::new();

            for tool_call in tool_calls.iter() {
                self.send(&agent.id, ChatResponse::ToolCallStart(tool_call.clone()))
                    .await?;
                if let Some(tool_result) = self.execute_tool(&agent.id, tool_call).await? {
                    tool_results.push(tool_result.clone());
                    self.send(&agent.id, ChatResponse::ToolCallEnd(tool_result))
                        .await?;
                }
            }

            context = context
                .add_message(ContextMessage::assistant(content, Some(tool_calls)))
                .add_tool_results(tool_results.clone());

            self.set_context(&agent.id, context.clone()).await?;

            if tool_results.is_empty() {
                break;
            }
        }

        self.complete_turn(&agent.id).await?;

        Ok(())
    }

    async fn init_agent(&self, agent_id: &AgentId) -> anyhow::Result<()> {
        let conversation_service = self.app.conversation_service();

        while let Some(event) = conversation_service
            .update(&self.conversation_id, |c| c.poll_event(agent_id))
            .await?
        {
            self.init_agent_with_event(agent_id, &event).await?;
        }

        Ok(())
    }
}
