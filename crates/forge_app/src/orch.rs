// Tests for this module can be found in: tests/orch_*.rs
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_recursion::async_recursion;
use derive_setters::Setters;
use forge_domain::{Agent, *};
use forge_template::Element;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::TemplateEngine;
use crate::agent::AgentService;
use crate::compact::Compactor;
use crate::title_generator::TitleGenerator;

#[derive(Clone, Setters)]
#[setters(into)]
pub struct Orchestrator<S> {
    services: Arc<S>,
    sender: Option<ArcSender>,
    conversation: Conversation,
    environment: Environment,
    tool_definitions: Vec<ToolDefinition>,
    models: Vec<Model>,
    agent: Agent,
    event: Event,
    error_tracker: ToolErrorTracker,
}

impl<S: AgentService> Orchestrator<S> {
    pub fn new(
        services: Arc<S>,
        environment: Environment,
        conversation: Conversation,
        agent: Agent,
        event: Event,
    ) -> Self {
        Self {
            conversation,
            environment,
            services,
            agent,
            event,
            sender: Default::default(),
            tool_definitions: Default::default(),
            models: Default::default(),
            error_tracker: Default::default(),
        }
    }

    /// Get a reference to the internal conversation
    pub fn get_conversation(&self) -> &Conversation {
        &self.conversation
    }

    // Helper function to get all tool results from a vector of tool calls
    #[async_recursion]
    async fn execute_tool_calls<'a>(
        &self,
        tool_calls: &[ToolCallFull],
        tool_context: &ToolCallContext,
    ) -> anyhow::Result<Vec<(ToolCallFull, ToolResult)>> {
        let agent = &self.agent;
        // Always process tool calls sequentially
        let mut tool_call_records = Vec::with_capacity(tool_calls.len());

        let system_tools = self
            .tool_definitions
            .iter()
            .map(|tool| &tool.name)
            .collect::<HashSet<_>>();

        for tool_call in tool_calls {
            // Send the start notification for system tools and not agent as a tool
            let is_system_tool = system_tools.contains(&tool_call.name);
            if is_system_tool {
                self.send(ChatResponse::ToolCallStart(tool_call.clone()))
                    .await?;
            }

            // Execute the tool
            let tool_result = self
                .services
                .call(agent, tool_context, tool_call.clone())
                .await;

            if tool_result.is_error() {
                warn!(
                    agent_id = %agent.id,
                    name = %tool_call.name,
                    arguments = %tool_call.arguments.to_owned().into_string(),
                    output = ?tool_result.output,
                    "Tool call failed",
                );
            }

            // Send the end notification for system tools and not agent as a tool
            if is_system_tool {
                self.send(ChatResponse::ToolCallEnd(tool_result.clone()))
                    .await?;
            }
            // Ensure all tool calls and results are recorded
            // Adding task completion records is critical for compaction to work correctly
            tool_call_records.push((tool_call.clone(), tool_result));
        }

        Ok(tool_call_records)
    }

    async fn send(&self, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(message)).await?
        }
        Ok(())
    }

    // Returns if agent supports tool or not.
    fn is_tool_supported(&self) -> anyhow::Result<bool> {
        let agent = &self.agent;
        let model_id = &agent.model;

        // Check if at agent level tool support is defined
        let tool_supported = match agent.tool_supported {
            Some(tool_supported) => tool_supported,
            None => {
                // If not defined at agent level, check model level

                let model = self.models.iter().find(|model| &model.id == model_id);
                model
                    .and_then(|model| model.tools_supported)
                    .unwrap_or_default()
            }
        };

        debug!(
            agent_id = %agent.id,
            model_id = %model_id,
            tool_supported,
            "Tool support check"
        );
        Ok(tool_supported)
    }

    async fn execute_chat_turn(
        &self,
        model_id: &ModelId,
        context: Context,
        reasoning_supported: bool,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let tool_supported = self.is_tool_supported()?;
        let mut transformers = DefaultTransformation::default()
            .pipe(SortTools::new())
            .pipe(TransformToolCalls::new().when(|_| !tool_supported))
            .pipe(ImageHandling::new())
            .pipe(DropReasoningDetails.when(|_| !reasoning_supported))
            .pipe(ReasoningNormalizer.when(|_| reasoning_supported));
        let response = self
            .services
            .chat_agent(
                model_id,
                transformers.transform(context),
                Some(self.agent.provider.clone()),
            )
            .await?;

        response.into_full(!tool_supported).await
    }
    /// Generates a RequestContext
    /// It just clones it, but would be nice to have
    /// it done explicitly or need to find a better approach.
    fn generate_request_context(&self, context: &Context) -> Context {
        context.clone()
    }

    /// Checks if compaction is needed and performs it if necessary
    fn check_and_compact(
        &self,
        context: &Context,
        metadata: &Option<CompactionMetadata>,
    ) -> anyhow::Result<Option<crate::compact::CompactionResult>> {
        let agent = &self.agent;
        // Estimate token count for compaction decision
        let token_count = context.token_count();
        if agent.should_compact(context, *token_count)
            && let Some(compact) = agent.compact.clone()
        {
            info!(agent_id = %agent.id, "Compaction needed");
            Compactor::new(compact, self.environment.clone())
                .compact(context.clone(), metadata.clone(), false)
                .map(Some)
        } else {
            debug!(agent_id = %agent.id, "Compaction not needed");
            Ok(None)
        }
    }

    // Create a helper method with the core functionality
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let event = self.event.clone();

        debug!(
            conversation_id = %self.conversation.id.clone(),
            event_value = %format!("{:?}", event.value),
            "Dispatching event"
        );

        debug!(
            conversation_id = %self.conversation.id,
            agent = %self.agent.id,
            event = ?event,
            "Initializing agent"
        );

        let model_id = self.get_model();

        let mut p_context = self.conversation.context.clone().unwrap_or_default();

        // Create agent reference for the rest of the method
        let agent = &self.agent;

        // Signals that the loop should suspend (task may or may not be completed)
        let mut should_yield = false;

        // Signals that the task is completed
        let mut is_complete = false;

        let mut request_count = 0;

        // Retrieve the number of requests allowed per tick.
        let max_requests_per_turn = agent.max_requests_per_turn;

        let tool_context =
            ToolCallContext::new(self.conversation.metrics.clone()).sender(self.sender.clone());

        // Asynchronously generate a title for the provided task
        // TODO: Move into app.rs
        let title = self.generate_title(model_id.clone());

        while !should_yield {
            // Set context for the current loop iteration
            self.conversation.context = Some(p_context.clone());
            self.services.update(self.conversation.clone()).await?;

            let message = crate::retry::retry_with_config(
                &self.environment.retry_config,
                || {
                    // Generate "RequestContext" from Context for provider call
                    let request_context = self.generate_request_context(&p_context);
                    self.execute_chat_turn(&model_id, request_context, p_context.is_reasoning_supported())
                },

                self.sender.as_ref().map(|sender| {
                    let sender = sender.clone();
                    let agent_id = agent.id.clone();
                    let model_id = model_id.clone();
                    move |error: &anyhow::Error, duration: Duration| {
                        let root_cause = error.root_cause();
                        tracing::error!(agent_id = %agent_id, error = ?root_cause, model=%model_id, "Retry Attempt");
                        let retry_event = ChatResponse::RetryAttempt {
                            cause: error.into(),
                            duration,
                        };
                        let _ = sender.try_send(Ok(retry_event));
                    }
                }),
            ).await?;

            // FIXME: Add a unit test in orch spec, to guarantee that compaction is
            // triggered after receiving the response Trigger compaction after
            // making a request NOTE: Ideally compaction should be implemented
            // as a transformer
            if let Some(c_context) =
                self.check_and_compact(&p_context.context, &p_context.compaction_metadata)?
            {
                info!(agent_id = %agent.id, "Using compacted context from execution");
                p_context = p_context
                    .context(c_context.context)
                    .compaction_metadata(c_context.metadata);
            } else {
                debug!(agent_id = %agent.id, "No compaction was needed");
            }

            info!(
                conversation_id = %self.conversation.id,
                conversation_length = p_context.messages.len(),
                token_usage = format!("{}", message.usage.prompt_tokens),
                total_tokens = format!("{}", message.usage.total_tokens),
                cached_tokens = format!("{}", message.usage.cached_tokens),
                cost = message.usage.cost.unwrap_or_default(),
                finish_reason = message.finish_reason.as_ref().map_or("", |reason| reason.into()),
                "Processing usage information"
            );

            debug!(agent_id = %agent.id, tool_call_count = message.tool_calls.len(), "Tool call count");

            // Turn is completed, if finish_reason is 'stop'. Gemini models return stop as
            // finish reason with tool calls.
            is_complete =
                message.finish_reason == Some(FinishReason::Stop) && message.tool_calls.is_empty();

            // Should yield if a tool is asking for a follow-up
            should_yield = is_complete
                || message
                    .tool_calls
                    .iter()
                    .any(|call| ToolCatalog::should_yield(&call.name));

            if let Some(reasoning) = message.reasoning.as_ref()
                && p_context.is_reasoning_supported()
            {
                // If reasoning is present, send it as a separate message
                self.send(ChatResponse::TaskReasoning { content: reasoning.to_string() })
                    .await?;
            }

            // Send the content message
            self.send(ChatResponse::TaskMessage {
                content: ChatResponseContent::Markdown(message.content.clone()),
            })
            .await?;

            // Process tool calls and update context
            let mut tool_call_records = self
                .execute_tool_calls(&message.tool_calls, &tool_context)
                .await?;

            self.error_tracker.adjust_record(&tool_call_records);
            let allowed_max_attempts = self.error_tracker.limit();
            for (_, result) in tool_call_records.iter_mut() {
                if result.is_error() {
                    let attempts_left = self.error_tracker.remaining_attempts(&result.name);
                    // Add attempt information to the error message so the agent can reflect on it.
                    let context = serde_json::json!({
                        "attempts_left": attempts_left,
                        "allowed_max_attempts": allowed_max_attempts,
                    });
                    let text = TemplateEngine::default()
                        .render("forge-tool-retry-message.md", &context)?;
                    let message = Element::new("retry").text(text);

                    result.output.combine_mut(ToolOutput::text(message));
                }
            }

            p_context.context = p_context.context.append_message(
                message.content.clone(),
                message.reasoning_details,
                message.usage,
                tool_call_records,
            );

            if self.error_tracker.limit_reached() {
                self.send(ChatResponse::Interrupt {
                    reason: InterruptionReason::MaxToolFailurePerTurnLimitReached {
                        limit: *self.error_tracker.limit() as u64,
                        errors: self.error_tracker.errors().clone(),
                    },
                })
                .await?;
                // Should yield if too many errors are produced
                should_yield = true;
            }

            // Update context in the conversation
            p_context.context = SetModel::new(model_id.clone()).transform(p_context.context);
            self.conversation.context = Some(p_context.clone());
            self.services.update(self.conversation.clone()).await?;
            request_count += 1;

            if !should_yield && let Some(max_request_allowed) = max_requests_per_turn {
                // Check if agent has reached the maximum request per turn limit
                if request_count >= max_request_allowed {
                    warn!(
                        agent_id = %agent.id,
                        model_id = %model_id,
                        request_count,
                        max_request_allowed,
                        "Agent has reached the maximum request per turn limit"
                    );
                    // raise an interrupt event to notify the UI
                    self.send(ChatResponse::Interrupt {
                        reason: InterruptionReason::MaxRequestPerTurnLimitReached {
                            limit: max_request_allowed as u64,
                        },
                    })
                    .await?;
                    // force completion
                    should_yield = true;
                }
            }

            // Update metrics in conversation
            tool_context.with_metrics(|metrics| {
                self.conversation.metrics = metrics.clone();
            })?;
        }

        // Set conversation title
        if let Some(title) = title.await.ok().flatten() {
            debug!(conversation_id = %self.conversation.id, title, "Title generated for conversation");
            self.conversation.title = Some(title)
        }

        self.services.update(self.conversation.clone()).await?;

        // Signal Task Completion
        if is_complete {
            self.send(ChatResponse::TaskComplete).await?;
        }

        Ok(())
    }

    fn get_model(&self) -> ModelId {
        self.agent.model.clone()
    }

    /// Creates a join handle which eventually resolves with the conversation
    /// title
    fn generate_title(&self, model: ModelId) -> JoinHandle<Option<String>> {
        let prompt = &self.event.value;
        if self.conversation.title.is_none()
            && let Some(prompt) = prompt.as_ref().and_then(|p| p.as_user_prompt())
        {
            let generator = TitleGenerator::new(
                self.services.clone(),
                prompt.to_owned(),
                model,
                Some(self.agent.provider.clone()),
            )
            .reasoning(self.agent.reasoning.clone());

            tokio::spawn(async move { generator.generate().await.ok().flatten() })
        } else {
            tokio::spawn(async { None })
        }
    }
}
