use std::cmp::max;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context as AnyhowContext;
use async_recursion::async_recursion;
use chrono::Local;
use derive_setters::Setters;
use forge_walker::Walker;
use futures::{Stream, StreamExt};
use handlebars::Handlebars;
use rust_embed::Embed;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::{find_compact_sequence, *};

/// Minimal trait that defines only the methods Orchestrator needs from services
#[async_trait::async_trait]
pub trait AgentService: Send + Sync + Clone + 'static {
    /// Perform a chat request with the provider
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;

    /// Execute a tool call
    async fn call(
        &self,
        agent: &Agent,
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;
}

#[derive(Embed)]
#[folder = "../../templates/"]
struct Templates;

/// Pure function to render templates without service dependency
fn render_template(template: &str, object: &impl serde::Serialize) -> anyhow::Result<String> {
    // Create handlebars instance with same configuration as ForgeTemplateService
    let mut hb = Handlebars::new();
    hb.set_strict_mode(true);
    hb.register_escape_fn(|str| str.to_string());

    // Register all partial templates
    hb.register_embed_templates::<Templates>()?;

    // Render the template
    let rendered = hb.render_template(template, object)?;
    Ok(rendered)
}

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
}

struct ChatCompletionResult {
    pub content: String,
    pub tool_calls: Vec<ToolCallFull>,
    pub usage: Usage,
}

impl<S: AgentService> Orchestrator<S> {
    pub fn new(services: Arc<S>, environment: Environment, conversation: Conversation) -> Self {
        Self {
            conversation,
            environment,
            services,
            sender: Default::default(),
            tool_definitions: Default::default(),
            models: Default::default(),
        }
    }

    /// Get a reference to the internal conversation
    pub fn get_conversation(&self) -> &Conversation {
        &self.conversation
    }

    // Helper function to get all tool results from a vector of tool calls
    #[async_recursion]
    async fn execute_tool_calls(
        &mut self,
        agent: &Agent,
        tool_calls: &[ToolCallFull],
        tool_context: &mut ToolCallContext,
    ) -> anyhow::Result<Vec<(ToolCallFull, ToolResult)>> {
        // Always process tool calls sequentially
        let mut tool_call_records = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            // Send the start notification
            self.send(agent, ChatResponse::ToolCallStart(tool_call.clone()))
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
            self.send(agent, ChatResponse::ToolCallEnd(tool_result.clone()))
                .await?;

            // Ensure all tool calls and results are recorded
            // Adding task completion records is critical for compaction to work correctly
            tool_call_records.push((tool_call.clone(), tool_result));
        }

        Ok(tool_call_records)
    }

    async fn send(&mut self, agent: &Agent, message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            // Send message if it's a Custom type or if hide_content is false
            let show_text = !agent.hide_content.unwrap_or_default();
            let can_send = !matches!(&message, ChatResponse::Text { .. }) || show_text;
            if can_send {
                sender.send(Ok(message)).await?
            }
        }
        Ok(())
    }

    /// Get the allowed tools for an agent
    fn get_allowed_tools(&mut self, agent: &Agent) -> anyhow::Result<Vec<ToolDefinition>> {
        if self.tool_definitions.is_empty() {
            // If no tools are defined, return an empty vector
            Ok(vec![])
        } else {
            let allowed = agent.tools.iter().flatten().collect::<HashSet<_>>();
            Ok(self
                .tool_definitions
                .iter()
                .filter(|tool| allowed.contains(&tool.name))
                .cloned()
                .collect())
        }
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
    fn is_tool_supported(&mut self, agent: &Agent) -> anyhow::Result<bool> {
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

    /// Apply compaction to the context if requested
    async fn compact_context(
        &mut self,
        agent: &Agent,
        context: Context,
    ) -> anyhow::Result<Context> {
        // Return early if agent doesn't have compaction configured
        if let Some(ref compact) = agent.compact {
            debug!(agent_id = %agent.id, "Context compaction triggered");

            // Identify and compress the first compressible sequence
            // Get all compressible sequences, considering the preservation window
            match find_compact_sequence(&context, compact.retention_window)
                .into_iter()
                .next()
            {
                Some(sequence) => {
                    debug!(agent_id = %agent.id, "Compressing sequence");
                    self.compress_single_sequence(compact, context, sequence)
                        .await
                }
                None => {
                    debug!(agent_id = %agent.id, "No compressible sequences found");
                    Ok(context)
                }
            }
        } else {
            Ok(context)
        }
    }

    /// Compress a single identified sequence of assistant messages
    async fn compress_single_sequence(
        &mut self,
        compact: &Compact,
        mut context: Context,
        sequence: (usize, usize),
    ) -> anyhow::Result<Context> {
        let (start, end) = sequence;

        // Extract the sequence to summarize
        let sequence_messages = &context.messages[start..=end].to_vec();

        // Generate summary for this sequence
        let summary = self
            .generate_summary_for_sequence(compact, sequence_messages)
            .await?;

        // Log the summary for debugging
        info!(
            summary = %summary,
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            sequence_length = sequence_messages.len(),
            "Created context compaction summary"
        );

        let summary = format!(
            r#"Continuing from a prior analysis. Below is a compacted summary of the ongoing session. Use this summary as authoritative context for your reasoning and decision-making. You do not need to repeat or reanalyze it unless specifically asked: <summary>{summary}</summary> Proceed based on this context.
        "#
        );

        // Replace the sequence with a single summary message using splice
        // This removes the sequence and inserts the summary message in-place
        context.messages.splice(
            start..=end,
            std::iter::once(ContextMessage::assistant(summary, None)),
        );

        Ok(context)
    }

    /// Generate a summary for a specific sequence of assistant messages
    async fn generate_summary_for_sequence(
        &mut self,
        compact: &Compact,
        messages: &[ContextMessage],
    ) -> anyhow::Result<String> {
        // Create a temporary context with just the sequence for summarization
        let sequence_context = messages
            .iter()
            .fold(Context::default(), |ctx, msg| ctx.add_message(msg.clone()));

        // Render the summarization prompt
        let summary_tag = compact.summary_tag.as_ref().cloned().unwrap_or_default();
        let ctx = serde_json::json!({
            "context": sequence_context.to_text(),
            "summary_tag": summary_tag
        });

        let prompt = render_template(
            compact
                .prompt
                .as_deref()
                .unwrap_or("{{> system-prompt-context-summarizer.hbs}}"),
            &ctx,
        )?;

        // Create a new context
        let mut context = Context::default()
            .add_message(ContextMessage::user(prompt, compact.model.clone().into()));

        // Set max_tokens for summary
        if let Some(max_token) = compact.max_tokens {
            context = context.max_tokens(max_token);
        }

        // Get summary from the provider
        let response = self.services.chat(&compact.model, context).await?;

        self.collect_completion_stream_content(compact, response)
            .await
    }

    /// Collects the content from a streaming ChatCompletionMessage response
    /// and extracts text within the configured tag if present
    async fn collect_completion_stream_content(
        &mut self,
        compact: &Compact,
        mut stream: impl Stream<Item = anyhow::Result<ChatCompletionMessage>> + std::marker::Unpin,
    ) -> anyhow::Result<String> {
        let mut result_content = String::new();

        while let Some(message_result) = stream.next().await {
            let message = message_result?;
            if let Some(content) = message.content {
                result_content.push_str(content.as_str());
            }
        }

        // Extract content from within configured tags if present and if tag is
        // configured
        // TODO: If no tag is found, it could be a misformed LLM call. We should
        // consider retrying.
        if let Some(extracted) = extract_tag_content(
            &result_content,
            compact
                .summary_tag
                .as_ref()
                .cloned()
                .unwrap_or_default()
                .as_str(),
        ) {
            return Ok(extracted.to_string());
        }

        // If no tag extraction performed, return the original content
        Ok(result_content)
    }

    async fn set_system_prompt(
        &mut self,
        context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<Context> {
        Ok(if let Some(system_prompt) = &agent.system_prompt {
            let env = self.environment.clone();
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

            let system_message = render_template(system_prompt.template.as_str(), &ctx)?;

            context.set_first_system_message(system_message)
        } else {
            context
        })
    }

    /// Process usage information from a chat completion message
    fn update_usage(
        &mut self,
        message: &ChatCompletionMessage,
        context: &Context,
        request_usage: Usage,
    ) -> Usage {
        // If usage information is provided by provider use that else depend on
        // estimates.

        let mut usage = message.usage.clone().unwrap_or(request_usage);
        let content_length = context.to_text().len();
        usage.estimated_tokens = estimate_token_count(content_length) as u64;
        usage.content_length = content_length as u64;
        usage
    }

    async fn collect_messages(
        &mut self,
        agent: &Agent,
        context: &Context,
        mut response: impl Stream<Item = anyhow::Result<ChatCompletionMessage>> + std::marker::Unpin,
    ) -> anyhow::Result<ChatCompletionResult> {
        let mut messages = Vec::new();
        let mut usage: Usage = Default::default();
        let mut content = String::new();
        let mut xml_tool_calls = None;
        let mut tool_interrupted = false;

        // Only interrupt the loop for XML tool calls if tool_supported is false
        let should_interrupt_for_xml = !self.is_tool_supported(agent)?;

        while let Some(message) = response.next().await {
            let message = message?;
            messages.push(message.clone());

            // Process usage information
            usage = self.update_usage(&message, context, usage);

            // Process content
            if let Some(content_part) = message.content.as_ref() {
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
        // TODO: Parse failure should be retried
        let partial_tool_calls = ToolCallFull::try_from_parts(&tool_call_parts)
            .with_context(|| format!("Failed to parse tool call: {tool_call_parts:?}"))?;

        // Combine all sources of tool calls
        let tool_calls: Vec<ToolCallFull> = initial_tool_calls
            .into_iter()
            .chain(partial_tool_calls)
            .chain(xml_tool_calls)
            .collect();

        Ok(ChatCompletionResult { content, tool_calls, usage })
    }

    pub async fn dispatch(&mut self, event: Event) -> anyhow::Result<()> {
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

    async fn chat(
        &mut self,
        agent: &Agent,
        model_id: &ModelId,
        context: Context,
    ) -> anyhow::Result<ChatCompletionResult> {
        let response = self.services.chat(model_id, context.clone()).await?;
        self.collect_messages(agent, &context, response).await
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

        // Reset all the available tools
        context = context.tools(self.get_allowed_tools(&agent)?);

        // Render the system prompts with the variables
        context = self.set_system_prompt(context, &agent, &variables).await?;

        // Render user prompts
        context = self.set_user_prompt(context, &agent, &variables, event)?;

        if let Some(temperature) = agent.temperature {
            context = context.temperature(temperature);
        }

        if let Some(top_p) = agent.top_p {
            context = context.top_p(top_p);
        }

        if let Some(top_k) = agent.top_k {
            context = context.top_k(top_k);
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
                        ContextMessage::user(content, model_id.clone().into())
                    }
                })
            });

        self.conversation.context = Some(context.clone());

        let mut tool_context = ToolCallContext::default().sender(self.sender.clone());

        let mut empty_tool_call_count = 0;

        while !tool_context.get_complete().await {
            // Set context for the current loop iteration
            self.conversation.context = Some(context.clone());

            let ChatCompletionResult { tool_calls, content, usage } =
                self.chat(&agent, &model_id, context.clone()).await?;

            // Send the usage information if available

            info!(
                token_usage = usage.prompt_tokens,
                estimated_token_usage = usage.estimated_tokens,
                content_length = usage.content_length,
                "Processing usage information"
            );
            self.send(&agent, ChatResponse::Usage(usage.clone()))
                .await?;

            // Check if context requires compression and decide to compact
            if agent.should_compact(&context, max(usage.prompt_tokens, usage.estimated_tokens)) {
                info!(agent_id = %agent.id, "Compaction needed, applying compaction");
                context = self.compact_context(&agent, context).await?;
            } else {
                debug!(agent_id = %agent.id, "Compaction not needed");
            }

            let empty_tool_calls = tool_calls.is_empty();

            debug!(agent_id = %agent.id, tool_call_count = tool_calls.len(), "Tool call count");

            // Process tool calls and update context
            context = context.append_message(
                content,
                model_id.clone(),
                self.execute_tool_calls(&agent, &tool_calls, &mut tool_context)
                    .await?,
                tool_supported,
            );

            if empty_tool_calls {
                // No tool calls present, which doesn't mean task is complete so reprompt the
                // agent to ensure the task complete.
                let content = render_template(
                    "{{> partial-tool-required.hbs}}",
                    &serde_json::json!({
                        "tool_supported": tool_supported
                    }),
                )?;
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
                    tool_context.set_complete().await;
                }
            } else {
                empty_tool_call_count = 0;
            }

            // Update context in the conversation
            self.conversation.context = Some(context.clone());
        }

        Ok(())
    }

    fn set_user_prompt(
        &mut self,
        mut context: Context,
        agent: &Agent,
        variables: &HashMap<String, Value>,
        event: &Event,
    ) -> anyhow::Result<Context> {
        let content = if let Some(user_prompt) = &agent.user_prompt {
            let event_context = EventContext::new(event.clone()).variables(variables.clone());
            debug!(event_context = ?event_context, "Event context");
            render_template(user_prompt.template.as_str(), &event_context)?
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_render_template_simple() {
        // Fixture: Create test data
        let data = json!({
            "name": "Forge",
            "version": "1.0",
            "features": ["templates", "rendering", "handlebars"]
        });

        // Actual: Render a simple template
        let template = "App: {{name}} v{{version}} - Features: {{#each features}}{{this}}{{#unless @last}}, {{/unless}}{{/each}}";
        let actual = render_template(template, &data).unwrap();

        // Expected: Result should match the expected string
        let expected = "App: Forge v1.0 - Features: templates, rendering, handlebars";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_template_with_partial() {
        // Fixture: Create test data
        let data = json!({
            "env": {
                "os": "test-os",
                "cwd": "/test/path",
                "shell": "/bin/test",
                "home": "/home/test"
            },
            "files": [
                "/file1.txt",
                "/file2.txt"
            ]
        });

        // Actual: Render the partial-system-info template
        let actual = render_template("{{> partial-system-info.hbs }}", &data).unwrap();

        // Expected: Result should contain the rendered system info with substituted
        // values
        assert!(actual.contains("<operating_system>test-os</operating_system>"));
    }
}
