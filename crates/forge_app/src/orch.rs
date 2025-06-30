use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
}

impl<S: AgentService> Orchestrator<S> {
    pub fn new(
        services: Arc<S>,
        environment: Environment,
        conversation: Conversation,
        current_time: chrono::DateTime<chrono::Local>,
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
        }
    }

    /// Get a reference to the internal conversation
    pub fn get_conversation(&self) -> &Conversation {
        &self.conversation
    }

    // Helper function to get all tool results from a vector of tool calls
    #[async_recursion]
    async fn execute_tool_calls(
        &self,
        agent: &Agent,
        tool_calls: &[ToolCallFull],
        tool_context: &mut ToolCallContext,
    ) -> anyhow::Result<Vec<(ToolCallFull, ToolResult)>> {
        // Always process tool calls sequentially
        let mut tool_call_records = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            // Send the start notification
            self.send(ChatResponse::ToolCallStart(tool_call.clone()))
                .await?;

            // Execute the tool
            let tool_result = self
                .services
                .call(agent, tool_context, tool_call.clone())
                .await;

            if tool_result.is_error() {
                warn!(
                    agent_id = %agent.id,
                    name = %tool_call.name,
                    arguments = %tool_call.arguments,
                    output = ?tool_result.output,
                    "Tool call failed",
                );
            }

            // Send the end notification
            self.send(ChatResponse::ToolCallEnd(tool_result.clone()))
                .await?;

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

            let current_time = self
                .current_time
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string();

            let tool_supported = self.is_tool_supported(agent)?;
            let supports_parallel_tool_calls = self.is_parallel_tool_call_supported(agent);
            let tool_information = match tool_supported {
                true => None,
                false => Some(ToolUsagePrompt::from(&self.get_allowed_tools(agent)?).to_string()),
            };

            let ctx = SystemContext {
                current_time,
                env: Some(env),
                tool_information,
                tool_supported,
                files,
                custom_rules: agent.custom_rules.as_ref().cloned().unwrap_or_default(),
                variables: variables.clone(),
                supports_parallel_tool_calls,
            };

            let system_message = self
                .services
                .render(system_prompt.template.as_str(), &ctx)
                .await?;

            context.set_first_system_message(system_message)
        } else {
            context
        })
    }

    pub async fn chat(&mut self, event: Event) -> anyhow::Result<()> {
        let target_agents = {
            debug!(
                conversation_id = %self.conversation.id.clone(),
                event_name = %event.name,
                event_value = %event.value,
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
    ) -> anyhow::Result<ChatCompletionMessageFull> {
        let mut transformers = TransformToolCalls::new()
            .when(|_| !tool_supported)
            .pipe(ImageHandling::new());
        let response = self
            .services
            .chat_agent(model_id, transformers.transform(context))
            .await?;
        response.into_full(!tool_supported).await
    }

    // Create a helper method with the core functionality
    async fn init_agent(&mut self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
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

        let mut context = self.conversation.context.clone().unwrap_or_default();

        // attach the conversation ID to the context
        context = context.conversation_id(self.conversation.id.clone());

        // Reset all the available tools
        context = context.tools(self.get_allowed_tools(&agent)?);

        // Render the system prompts with the variables
        context = self.set_system_prompt(context, &agent, &variables).await?;

        // Create a new compactor
        let compactor = Compactor::new(self.services.clone());

        // Perform an aggressive compaction before starting out
        // TODO: Improve compaction by providing context of the user message
        context = compactor.compact(&agent, context, true).await?;

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

        // Process attachments from the event if they exist
        let attachments = event.attachments.clone();

        // Process each attachment and fold the results into the context
        context = attachments
            .into_iter()
            .fold(context.clone(), |ctx, attachment| {
                ctx.add_message(match attachment.content {
                    AttachmentContent::Image(image) => ContextMessage::Image(image),
                    AttachmentContent::FileContent(content) => {
                        let elm = Element::new("file_content")
                            .attr("path", attachment.path)
                            .attr("start_line", 1)
                            .attr("end_line", content.lines().count())
                            .attr("total_lines", content.lines().count())
                            .cdata(content);

                        ContextMessage::user(elm, model_id.clone().into())
                    }
                })
            });

        self.conversation.context = Some(context.clone());

        // Indicates whether the tool execution has been completed
        let mut is_complete = false;

        let mut empty_tool_call_count = 0;
        let is_tool_supported = self.is_tool_supported(&agent)?;

        while !is_complete {
            // Set context for the current loop iteration
            self.conversation.context = Some(context.clone());
            self.services.update(self.conversation.clone()).await?;

            let ChatCompletionMessageFull { tool_calls, content, mut usage } =
                crate::retry::retry_with_config(&self.environment.retry_config, || {
                    self.execute_chat_turn(&model_id, context.clone(), is_tool_supported)
                })
                .await?;

            // Set estimated tokens
            usage.estimated_tokens = context.token_count();

            // Send the usage information if available

            info!(
                token_usage = usage.prompt_tokens,
                estimated_token_usage = usage.estimated_tokens,
                "Processing usage information"
            );

            self.send(ChatResponse::Usage(usage.clone())).await?;

            // Check if context requires compression and decide to compact
            let context_tokens = usage
                .prompt_tokens
                .max(usage.estimated_tokens.max(usage.total_tokens));
            if agent.should_compact(&context, context_tokens) {
                info!(agent_id = %agent.id, "Compaction needed, applying compaction");
                context = compactor.compact(&agent, context, false).await?;
            } else {
                debug!(agent_id = %agent.id, "Compaction not needed");
            }

            let has_no_tool_calls = tool_calls.is_empty();

            debug!(agent_id = %agent.id, tool_call_count = tool_calls.len(), "Tool call count");

            is_complete = tool_calls.iter().any(|call| Tools::is_complete(&call.name));

            if !is_complete && !has_no_tool_calls {
                // If task is completed we would have already displayed a message so we can
                // ignore the content that's collected from the stream
                // NOTE: Important to send the content messages before the tool call happens
                self.send(ChatResponse::Text {
                    text: remove_tag_with_prefix(&content, "forge_")
                        .as_str()
                        .to_string(),
                    is_complete: true,
                    is_md: true,
                    is_summary: false,
                })
                .await?;
            }
            let mut tool_context =
                ToolCallContext::new(self.conversation.tasks.clone()).sender(self.sender.clone());

            // Process tool calls and update context
            context = context.append_message(
                content.clone(),
                self.execute_tool_calls(&agent, &tool_calls, &mut tool_context)
                    .await?,
            );
            self.conversation.tasks = tool_context.tasks;

            context = SetModel::new(model_id.clone()).transform(context);

            if has_no_tool_calls {
                // No tool calls present, which doesn't mean task is complete so reprompt the
                // agent to ensure the task complete.
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
                if empty_tool_call_count > 3 {
                    warn!(
                        agent_id = %agent.id,
                        model_id = %model_id,
                        empty_tool_call_count,
                        "Forced completion due to repeated empty tool calls"
                    );
                }
            } else {
                empty_tool_call_count = 0;
            }

            // Update context in the conversation
            self.conversation.context = Some(context.clone());
            self.services.update(self.conversation.clone()).await?;
        }

        Ok(())
    }

    async fn set_user_prompt(
        &mut self,
        mut context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
        event: &Event,
    ) -> anyhow::Result<Context> {
        let content = if let Some(user_prompt) = &agent.user_prompt {
            let event_context = EventContext::new(event.clone())
                .variables(variables.clone())
                .current_time(
                    self.current_time
                        .format("%Y-%m-%d %H:%M:%S %:z")
                        .to_string(),
                );
            debug!(event_context = ?event_context, "Event context");
            self.services
                .render(user_prompt.template.as_str(), &event_context)
                .await?
        } else {
            // Use the raw event value as content if no user_prompt is provided
            event.value.to_string()
        };

        if !content.is_empty() {
            context = context.add_message(ContextMessage::user(content, agent.model.clone()));
        }

        Ok(context)
    }
}
