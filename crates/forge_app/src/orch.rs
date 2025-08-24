// Tests for this module can be found in: tests/orch_*.rs
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_recursion::async_recursion;
use derive_setters::Setters;
use forge_domain::*;
use forge_template::Element;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::agent::AgentService;
use crate::compact::Compactor;
pub type ArcSender = Arc<tokio::sync::mpsc::Sender<anyhow::Result<ChatResponse>>>;

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
}

impl<S: AgentService> Orchestrator<S> {
    pub fn new(
        services: Arc<S>,
        environment: Environment,
        conversation: Conversation,
        current_time: chrono::DateTime<chrono::Local>,
        custom_instructions: Vec<String>,
    ) -> Self {
        Self {
            conversation,
            environment,
            services,
            sender: Default::default(),
            tool_definitions: Default::default(),
            models: Default::default(),
            files: Default::default(),
            current_time,
            custom_instructions,
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
        agent: &Agent,
        tool_calls: &[ToolCallFull],
        tool_context: &ToolCallContext,
    ) -> anyhow::Result<Vec<(ToolCallFull, ToolResult)>> {
        // Always process tool calls sequentially
        let mut tool_call_records = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            let is_agent = self
                .conversation
                .agents
                .iter()
                .any(|agent| agent.id.as_str() == tool_call.name.as_str());

            // Send the start notification
            if !is_agent {
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

            // Send the end notification
            if !is_agent {
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

    /// Get the allowed tools for an agent
    fn get_allowed_tools(&mut self, agent: &Agent) -> anyhow::Result<Vec<ToolDefinition>> {
        let completion = ToolsDiscriminants::ForgeToolAttemptCompletion;
        let mut tools = vec![];
        if !self.tool_definitions.is_empty() {
            let allowed = agent.tools.iter().flatten().collect::<HashSet<_>>();
            tools.extend(
                self.tool_definitions
                    .iter()
                    .filter(|tool| tool.name != completion.name())
                    .filter(|tool| allowed.contains(&tool.name))
                    .cloned(),
            );
        }

        tools.push(completion.definition());

        Ok(tools)
    }

    /// Checks if parallel tool calls is supported by agent
    fn is_parallel_tool_call_supported(&self, agent: &Agent) -> bool {
        agent
            .model
            .as_ref()
            .and_then(|model_id| self.models.iter().find(|model| &model.id == model_id))
            .and_then(|model| model.supports_parallel_tool_calls)
            .unwrap_or_default()
    }

    // Returns if agent supports tool or not.
    fn is_tool_supported(&self, agent: &Agent) -> anyhow::Result<bool> {
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

    fn is_reasoning_supported(&self, agent: &Agent) -> anyhow::Result<bool> {
        let model_id = agent
            .model
            .as_ref()
            .ok_or(Error::MissingModel(agent.id.clone()))?;

        let model = self.models.iter().find(|model| &model.id == model_id);
        let reasoning_supported = model
            .and_then(|model| model.supports_reasoning)
            .unwrap_or_default();

        debug!(
            agent_id = %agent.id,
            model_id = %model_id,
            reasoning_supported,
            "Reasoning support check"
        );
        Ok(reasoning_supported)
    }

    async fn set_system_prompt(
        &mut self,
        context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<Context> {
        Ok(if let Some(system_prompt) = &agent.system_prompt {
            let env = self.environment.clone();
            let mut files = self.files.clone();
            files.sort();

            let tool_supported = self.is_tool_supported(agent)?;
            let supports_parallel_tool_calls = self.is_parallel_tool_call_supported(agent);
            let tool_information = match tool_supported {
                true => None,
                false => Some(ToolUsagePrompt::from(&self.get_allowed_tools(agent)?).to_string()),
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
                variables: variables.clone(),
                supports_parallel_tool_calls,
            };

            let static_block = self.services.render(&system_prompt.template, &()).await?;
            let non_static_block = self
                .services
                .render("{{> forge-custom-agent-template.hbs }}", &ctx)
                .await?;

            context.set_system_messages(vec![static_block, non_static_block])
        } else {
            context
        })
    }

    pub async fn chat(&mut self, event: Event) -> anyhow::Result<()> {
        let target_agents = {
            debug!(
                conversation_id = %self.conversation.id.clone(),
                event_name = %event.name,
                event_value = %format!("{:?}", event.value),
                "Dispatching event"
            );
            self.conversation.dispatch_event(event.clone())
        };

        // Execute all agent initialization with the event
        for agent_id in &target_agents {
            self.init_agent(agent_id, &event).await?;
        }

        Ok(())
    }

    async fn execute_chat_turn(
        &self,
        model_id: &ModelId,
        context: Context,
        tool_supported: bool,
        reasoning_supported: bool,
    ) -> anyhow::Result<ChatCompletionMessageFull> {
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
    async fn check_and_compact(
        &self,
        agent: &Agent,
        context: &Context,
    ) -> anyhow::Result<Option<Context>> {
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
    async fn init_agent(&mut self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
        let mut tool_failure_attempts = HashMap::new();
        let variables = self.conversation.variables.clone();
        debug!(
            conversation_id = %self.conversation.id,
            agent = %agent_id,
            event = ?event,
            "Initializing agent"
        );
        let agent = self.conversation.get_agent(agent_id)?.clone();
        let model_id = agent
            .model
            .clone()
            .ok_or(Error::MissingModel(agent.id.clone()))?;
        let tool_supported = self.is_tool_supported(&agent)?;
        let reasoning_supported = self.is_reasoning_supported(&agent)?;

        let mut context = self.conversation.context.clone().unwrap_or_default();

        // attach the conversation ID to the context
        context = context.conversation_id(self.conversation.id);

        // Reset all the available tools
        context = context.tools(self.get_allowed_tools(&agent)?);

        // Render the system prompts with the variables
        context = self.set_system_prompt(context, &agent, &variables).await?;

        // Render user prompts
        context = self
            .set_user_prompt(context, &agent, &variables, event)
            .await?;

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

        if reasoning_supported {
            // Add reasoning specific params to context only if reasoning is supported
            // by underlying model
            if let Some(reasoning) = agent.reasoning.as_ref() {
                context = context.reasoning(reasoning.clone());
            }
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

        let mut empty_tool_call_count = 0;
        let mut request_count = 0;

        // Retrieve the number of requests allowed per tick.
        let max_requests_per_turn = self.conversation.max_requests_per_turn;

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
                || self.execute_chat_turn(&model_id, context.clone(), tool_supported, reasoning_supported),
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
            ) = tokio::try_join!(main_request, self.check_and_compact(&agent, &context))?;

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
                && reasoning_supported
            {
                // If reasoning is present, send it as a separate message
                self.send(ChatResponse::TaskReasoning { content: reasoning.to_string() })
                    .await?;
            }

            // Check if tool calls are within allowed limits if max_tool_failure_per_turn is
            // configured
            let mut allowed_limits_exceeded =
                self.check_tool_call_failures(&tool_failure_attempts, &tool_calls);

            // Process tool calls and update context
            let mut tool_call_records = self
                .execute_tool_calls(&agent, &tool_calls, &tool_context)
                .await?;

            // Update the tool call attempts, if the tool call is an error
            // we increment the attempts, otherwise we remove it from the attempts map
            if let Some(allowed_max_attempts) = self.conversation.max_tool_failure_per_turn.as_ref()
            {
                tool_call_records.iter_mut().for_each(|(_, result)| {
                    if result.is_error() {
                        let current_attempts = tool_failure_attempts
                            .entry(result.name.clone())
                            .and_modify(|count| *count += 1)
                            .or_insert(1);
                        let attempts_left = allowed_max_attempts.saturating_sub(*current_attempts);

                        // Add attempt information to the error message so the agent can reflect on it.
                        let message = Element::new("retry").text(format!(
                            "This tool call failed. You have {attempts_left} attempt(s) remaining out of a maximum of {allowed_max_attempts}. Please reflect on the error, adjust your approach if needed, and try again."
                        ));

                        result.output.combine_mut(ToolOutput::text(message));
                    } else {
                        tool_failure_attempts.remove(&result.name);
                    }
                });
            }

            context = context.append_message(content.clone(), reasoning_details, tool_call_records);

            if !(turn_has_tool_calls || has_tool_calls) {
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
                is_complete = true
            } else if turn_has_tool_calls && !has_tool_calls {
                // Since no tool calls are present, which doesn't mean task is complete so
                // re-prompt the agent to ensure the task complete.
                let content = self
                    .services
                    .render(
                        "{{> forge-partial-tool-required.hbs}}",
                        &serde_json::json!({
                            "tool_supported": tool_supported
                        }),
                    )
                    .await?;
                context =
                    context.add_message(ContextMessage::user(content, model_id.clone().into()));

                warn!(
                    agent_id = %agent.id,
                    model_id = %model_id,
                    empty_tool_call_count,
                    "Agent is unable to follow instructions"
                );

                empty_tool_call_count += 1;
                // TODO: Move the hard coded limit into env
                if empty_tool_call_count >= 3 {
                    warn!(
                        agent_id = %agent.id,
                        model_id = %model_id,
                        empty_tool_call_count,
                        "Forced completion due to repeated empty tool calls"
                    );
                    allowed_limits_exceeded = true;
                }
            } else {
                empty_tool_call_count = 0;
            }

            if allowed_limits_exceeded {
                // Tool call retry limit exceeded, force completion
                warn!(
                    agent_id = %agent.id,
                    model_id = %model_id,
                    tools = %tool_failure_attempts.iter().map(|(name, count)| format!("{name}: {count}")).collect::<Vec<_>>().join(", "),
                    max_tool_failure_per_turn = ?self.conversation.max_tool_failure_per_turn,
                    "Tool execution failure limit exceeded - terminating conversation to prevent infinite retry loops."
                );

                if let Some(limit) = self.conversation.max_tool_failure_per_turn {
                    self.send(ChatResponse::Interrupt {
                        reason: InterruptionReason::MaxToolFailurePerTurnLimitReached {
                            limit: limit as u64,
                        },
                    })
                    .await?;
                }

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
        // Signal Task Completion
        if has_attempted_completion {
            self.send(ChatResponse::TaskComplete).await?;
        }

        Ok(())
    }

    fn check_tool_call_failures(
        &self,
        tool_failure_attempts: &HashMap<ToolName, usize>,
        tool_calls: &[ToolCallFull],
    ) -> bool {
        self.conversation
            .max_tool_failure_per_turn
            .is_some_and(|limit| {
                tool_calls
                    .iter()
                    .map(|call| tool_failure_attempts.get(&call.name).unwrap_or(&0))
                    .any(|count| *count >= limit)
            })
    }

    async fn set_user_prompt(
        &mut self,
        mut context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
        event: &Event,
    ) -> anyhow::Result<Context> {
        let content = if let Some(user_prompt) = &agent.user_prompt
            && event.value.is_some()
        {
            let event_context = EventContext::new(event.clone())
                .variables(variables.clone())
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
