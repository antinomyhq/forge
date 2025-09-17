// Tests for this module can be found in: tests/orch_*.rs
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_recursion::async_recursion;
use derive_setters::Setters;
use forge_domain::*;
use forge_template::Element;
use tracing::{debug, info, warn};

use crate::agent::AgentService;
use crate::compact::Compactor;
use crate::title_generator::TitleGenerator;

#[derive(Clone, Setters)]
#[setters(into, strip_option)]
pub struct Orchestrator<S> {
    services: Arc<S>,
    sender: Option<ArcSender>,
    conversation: Conversation,
    environment: Environment,
    tool_definitions: Vec<ToolDefinition>,
    models: Vec<Model>,
    files: Vec<String>,
    current_time: chrono::DateTime<chrono::Local>,
    custom_instructions: Vec<String>,
    agent: Agent,
    event: Event,
    error_tracker: ToolErrorTracker,
}

impl<S: AgentService> Orchestrator<S> {
    pub fn new(
        services: Arc<S>,
        environment: Environment,
        conversation: Conversation,
        current_time: chrono::DateTime<chrono::Local>,
        agent: Agent,
        event: Event,
    ) -> Self {
        Self {
            conversation,
            environment,
            services,
            current_time,
            agent,
            event,
            sender: Default::default(),
            tool_definitions: Default::default(),
            models: Default::default(),
            files: Default::default(),
            custom_instructions: Default::default(),
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

    /// Checks if parallel tool calls is supported by agent
    fn is_parallel_tool_call_supported(&self) -> bool {
        let agent = &self.agent;
        agent
            .model
            .as_ref()
            .and_then(|model_id| self.models.iter().find(|model| &model.id == model_id))
            .and_then(|model| model.supports_parallel_tool_calls)
            .unwrap_or_default()
    }

    // Returns if agent supports tool or not.
    fn is_tool_supported(&self) -> anyhow::Result<bool> {
        let agent = &self.agent;
        let model_id = agent
            .model
            .as_ref()
            .ok_or(Error::MissingModel(agent.id.clone()))?;

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

    async fn set_system_prompt(&self, context: Context) -> anyhow::Result<Context> {
        let agent = &self.agent;
        Ok(if let Some(system_prompt) = &agent.system_prompt {
            let env = self.environment.clone();
            let mut files = self.files.clone();
            files.sort();

            let tool_supported = self.is_tool_supported()?;
            let supports_parallel_tool_calls = self.is_parallel_tool_call_supported();
            let tool_information = match tool_supported {
                true => None,
                false => Some(ToolUsagePrompt::from(&self.tool_definitions).to_string()),
            };

            let mut custom_rules = Vec::new();

            agent.custom_rules.iter().for_each(|rule| {
                custom_rules.push(rule.as_str());
            });

            self.custom_instructions.iter().for_each(|rule| {
                custom_rules.push(rule.as_str());
            });

            let ctx = SystemContext {
                env: Some(env),
                tool_information,
                tool_supported,
                files,
                custom_rules: custom_rules.join("\n\n"),
                supports_parallel_tool_calls,
            };

            let static_block = self.services.render(&system_prompt.template, &()).await?;
            let non_static_block = self
                .services
                .render("{{> forge-custom-agent-template.md }}", &ctx)
                .await?;

            context.set_system_messages(vec![static_block, non_static_block])
        } else {
            context
        })
    }

    async fn execute_chat_turn(
        &self,
        model_id: &ModelId,
        context: Context,
        reasoning_supported: bool,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let tool_supported = self.is_tool_supported()?;
        let mut transformers = TransformToolCalls::new()
            .when(|_| !tool_supported)
            .pipe(ImageHandling::new())
            .pipe(DropReasoningDetails.when(|_| !reasoning_supported))
            .pipe(ReasoningNormalizer.when(|_| reasoning_supported));
        let response = self
            .services
            .chat_agent(model_id, transformers.transform(context))
            .await?;

        response.into_full(!tool_supported).await
    }
    /// Checks if compaction is needed and performs it if necessary
    async fn check_and_compact(&self, context: &Context) -> anyhow::Result<Option<Context>> {
        let agent = &self.agent;
        // Estimate token count for compaction decision
        let token_count = context.token_count();
        if agent.should_compact(context, *token_count) {
            info!(agent_id = %agent.id, "Compaction needed");
            Compactor::new(self.services.clone())
                .compact(agent, context.clone(), false)
                .await
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
            event_name = %event.name,
            event_value = %format!("{:?}", event.value),
            "Dispatching event"
        );

        debug!(
            conversation_id = %self.conversation.id,
            agent = %self.agent.id,
            event = ?event,
            "Initializing agent"
        );

        let model_id = self
            .agent
            .model
            .clone()
            .ok_or(Error::MissingModel(self.agent.id.clone()))?;
        let tool_supported = self.is_tool_supported()?;

        let mut context = self.conversation.context.clone().unwrap_or_default();

        // attach the conversation ID to the context
        context = context.conversation_id(self.conversation.id);

        // Reset all the available tools
        context = context.tools(self.tool_definitions.clone());

        // Render the system prompts with the variables
        context = self.set_system_prompt(context).await?;

        // Render user prompts
        context = self.set_user_prompt(context).await?;

        // Create agent reference for the rest of the method
        let agent = &self.agent;

        if let Some(temperature) = agent.temperature {
            context = context.temperature(temperature);
        }

        if let Some(top_p) = agent.top_p {
            context = context.top_p(top_p);
        }

        if let Some(top_k) = agent.top_k {
            context = context.top_k(top_k);
        }

        if let Some(max_tokens) = agent.max_tokens {
            context = context.max_tokens(max_tokens.value() as usize);
        }

        if let Some(reasoning) = agent.reasoning.as_ref() {
            context = context.reasoning(reasoning.clone());
        }

        // Process attachments from the event if they exist
        let attachments = event.attachments.clone();

        // Process each attachment and fold the results into the context
        context = attachments
            .into_iter()
            .fold(context.clone(), |ctx, attachment| {
                ctx.add_message(match attachment.content {
                    AttachmentContent::Image(image) => ContextMessage::Image(image),
                    AttachmentContent::FileContent {
                        content,
                        start_line,
                        end_line,
                        total_lines,
                    } => {
                        let elm = Element::new("file_content")
                            .attr("path", attachment.path)
                            .attr("start_line", start_line)
                            .attr("end_line", end_line)
                            .attr("total_lines", total_lines)
                            .cdata(content);

                        ContextMessage::user(elm, model_id.clone().into())
                    }
                })
            });

        // Indicates whether the tool execution has been completed
        let mut is_complete = false;
        let mut has_attempted_completion = false;

        let mut request_count = 0;

        // Retrieve the number of requests allowed per tick.
        let max_requests_per_turn = agent.max_requests_per_turn;

        // Store tool calls at turn level
        let mut turn_has_tool_calls = false;

        let tool_context =
            ToolCallContext::new(self.conversation.metrics.clone()).sender(self.sender.clone());
        while !is_complete {
            // Set context for the current loop iteration
            self.conversation.context = Some(context.clone());
            self.services.update(self.conversation.clone()).await?;

            // Run the main chat request and compaction check in parallel
            let main_request = crate::retry::retry_with_config(
                &self.environment.retry_config,
                || self.execute_chat_turn(&model_id, context.clone(), context.is_reasoning_supported()),
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
            );

            // Generate title only if conversation doesn't have any title and event.value
            // exists
            use futures::future::{Either, ready};
            let title_generator_future: Either<_, _> = if let Some(ref prompt) = self.event.value {
                if self.conversation.title.is_none() {
                    let title_generator = TitleGenerator::new(
                        self.services.clone(),
                        prompt.to_owned(),
                        model_id.clone(),
                    )
                    .reasoning(agent.reasoning.clone());
                    Either::Left(async move { title_generator.generate().await })
                } else {
                    Either::Right(ready(Ok::<Option<String>, anyhow::Error>(None)))
                }
            } else {
                Either::Right(ready(Ok::<Option<String>, anyhow::Error>(None)))
            };

            // Prepare compaction task that runs in parallel
            // Execute both operations in parallel
            let (
                ChatCompletionMessageFull {
                    tool_calls,
                    content,
                    usage,
                    reasoning,
                    reasoning_details,
                    finish_reason,
                },
                compaction_result,
                conversation_title,
            ) = tokio::try_join!(
                main_request,
                self.check_and_compact(&context),
                title_generator_future
            )?;

            // If conversation_title is generated then update the conversation with it's
            // title.
            if let Some(title) = conversation_title {
                debug!(conversation_id = %self.conversation.id, title, "Title generated for conversation");
                self.conversation.title = Some(title);
            }

            // Apply compaction result if it completed successfully
            match compaction_result {
                Some(compacted_context) => {
                    info!(agent_id = %agent.id, "Using compacted context from execution");
                    context = compacted_context;
                }
                None => {
                    debug!(agent_id = %agent.id, "No compaction was needed");
                }
            }

            info!(
                conversation_id = %self.conversation.id,
                conversation_length = context.messages.len(),
                token_usage = format!("{}", usage.prompt_tokens),
                total_tokens = format!("{}", usage.total_tokens),
                cached_tokens = format!("{}", usage.cached_tokens),
                cost = usage.cost.unwrap_or_default(),
                finish_reason = finish_reason.map_or("", |reason| reason.into()),
                "Processing usage information"
            );

            // Send the usage information if available
            self.send(ChatResponse::Usage(usage.clone())).await?;

            context = context.usage(usage);

            let has_tool_calls = !tool_calls.is_empty();
            has_attempted_completion = tool_calls
                .iter()
                .any(|call| Tools::is_attempt_completion(&call.name));

            debug!(agent_id = %agent.id, tool_call_count = tool_calls.len(), "Tool call count");

            // Turn is completed, if tool should yield
            is_complete = tool_calls
                .iter()
                .any(|call| Tools::should_yield(&call.name));

            if !is_complete && has_tool_calls {
                // If task is completed we would have already displayed a message so we can
                // ignore the content that's collected from the stream
                // NOTE: Important to send the content messages before the tool call happens
                self.send(ChatResponse::TaskMessage {
                    content: ChatResponseContent::Markdown(
                        remove_tag_with_prefix(&content, "forge_")
                            .as_str()
                            .to_string(),
                    ),
                })
                .await?;
            }

            if let Some(reasoning) = reasoning.as_ref()
                && !is_complete
                && context.is_reasoning_supported()
            {
                // If reasoning is present, send it as a separate message
                self.send(ChatResponse::TaskReasoning { content: reasoning.to_string() })
                    .await?;
            }

            // Process tool calls and update context
            let mut tool_call_records = self.execute_tool_calls(&tool_calls, &tool_context).await?;

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
                    let text = self
                        .services
                        .render("{{> forge-tool-retry-message.md }}", &context)
                        .await?;
                    let message = Element::new("retry").text(text);

                    result.output.combine_mut(ToolOutput::text(message));
                }
            }

            context = context.append_message(content.clone(), reasoning_details, tool_call_records);

            match (turn_has_tool_calls, has_tool_calls) {
                (false, false) => {
                    // No tools were called in the previous turn nor were they called in this step;
                    // Means that this is conversation.

                    self.send(ChatResponse::TaskMessage {
                        content: ChatResponseContent::Markdown(
                            remove_tag_with_prefix(&content, "forge_")
                                .as_str()
                                .to_string(),
                        ),
                    })
                    .await?;
                    is_complete = true;
                    self.error_tracker
                        .succeed(&ToolsDiscriminants::AttemptCompletion.name());
                }
                (true, false) => {
                    // Since no tool calls are present, which doesn't mean task is complete so
                    // re-prompt the agent to ensure the task complete.
                    let content = self.attempt_completion_prompt(tool_supported).await?;
                    let message = ContextMessage::user(content, model_id.clone().into());
                    context = context.add_message(message);
                    self.error_tracker
                        .failed(&ToolsDiscriminants::AttemptCompletion.name());
                }
                _ => {
                    self.error_tracker
                        .succeed(&ToolsDiscriminants::AttemptCompletion.name());
                }
            }

            if self.error_tracker.limit_reached() {
                self.send(ChatResponse::Interrupt {
                    reason: InterruptionReason::MaxToolFailurePerTurnLimitReached {
                        limit: *self.error_tracker.limit() as u64,
                        errors: self.error_tracker.errors().clone(),
                    },
                })
                .await?;

                is_complete = true;
            }

            // Update context in the conversation
            context = SetModel::new(model_id.clone()).transform(context);
            self.conversation.context = Some(context.clone());
            self.services.update(self.conversation.clone()).await?;
            request_count += 1;

            if !is_complete && let Some(max_request_allowed) = max_requests_per_turn {
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
                    is_complete = true;
                }
            }

            // Update if turn has tool calls
            turn_has_tool_calls = turn_has_tool_calls || has_tool_calls;
        }

        // Update metrics in conversation
        tool_context.with_metrics(|metrics| {
            self.conversation.metrics = metrics.clone();
        })?;
        self.services.update(self.conversation.clone()).await?;

        // Signal Task Completion
        if has_attempted_completion {
            self.send(ChatResponse::TaskComplete).await?;
        }

        Ok(())
    }

    async fn attempt_completion_prompt(&self, tool_supported: bool) -> anyhow::Result<String> {
        let ctx = serde_json::json!({"tool_supported": tool_supported});
        self.services
            .render("{{> forge-partial-tool-required.md}}", &ctx)
            .await
    }

    async fn set_user_prompt(&self, mut context: Context) -> anyhow::Result<Context> {
        let agent = &self.agent;
        let event = &self.event;
        let content = if let Some(user_prompt) = &agent.user_prompt
            && event.value.is_some()
        {
            let event_context = EventContext::new(event.clone())
                .current_time(self.current_time.format("%Y-%m-%d").to_string());
            debug!(event_context = ?event_context, "Event context");
            Some(
                self.services
                    .render(user_prompt.template.as_str(), &event_context)
                    .await?,
            )
        } else {
            // Use the raw event value as content if no user_prompt is provided
            event.value.as_ref().map(|v| v.to_string())
        };

        if let Some(content) = content {
            context = context.add_message(ContextMessage::user(content, agent.model.clone()));
        }

        Ok(context)
    }
}
