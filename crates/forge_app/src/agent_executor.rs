use std::sync::Arc;

use convert_case::{Case, Casing};
use forge_domain::{
    AgentId, ChatRequest, ChatResponse, ChatResponseContent, ContextMessage, Conversation, Event,
    EventData, Hook, RequestPayload, Role, TextMessage, TitleFormat,
    ToolCallContext, ToolcallEndPayload, ToolDefinition, ToolName, ToolOutput, ToolResult,
};
use forge_template::Element;
use futures::StreamExt;
use tokio::sync::{Mutex, RwLock};

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
        ctx.send_tool_input(
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
        let captured_output = Arc::new(Mutex::new(None));
        let app = crate::ForgeApp::<S>::new(self.services.clone());
        let app = if agent_id.is_codebase_search() {
            let hook = codebase_search_hook(
                env.codebase_search_max_iterations,
                captured_output.clone(),
            );
            app.with_hook(Arc::new(hook))
        } else {
            app
        };

        let request = ChatRequest::new(Event::new(task.clone()), conversation.id);
        let mut response_stream = app.chat(agent_id.clone(), request).await?;

        // Collect responses from the agent
        let mut output = String::new();
        while let Some(message) = response_stream.next().await {
            let message = message?;
            if matches!(
                &message,
                ChatResponse::ToolCallStart(_) | ChatResponse::ToolCallEnd(_)
            ) {
                output.clear();
            }
            match message {
                ChatResponse::TaskMessage { ref content } => match content {
                    ChatResponseContent::ToolInput(_) => ctx.send(message).await?,
                    ChatResponseContent::ToolOutput(_) => {}
                    ChatResponseContent::Markdown { text, partial } => {
                        if *partial {
                            output.push_str(text);
                        } else {
                            output = text.to_string();
                        }
                    }
                },
                ChatResponse::TaskReasoning { .. } => {}
                ChatResponse::TaskComplete => {}
                ChatResponse::ToolCallStart(_) => ctx.send(message).await?,
                ChatResponse::ToolCallEnd(_) => ctx.send(message).await?,
                ChatResponse::RetryAttempt { .. } => ctx.send(message).await?,
                ChatResponse::Interrupt { .. } => ctx.send(message).await?,
            }
        }

        // Prefer the captured tool result output, fall back to text output
        if let Some(result) = captured_output.lock().await.take() {
            Ok(ToolOutput::ai(
                conversation.id,
                result.output.as_str().unwrap_or(""),
            ))
        } else if !output.is_empty() {
            // Create tool output
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

/// Manages iteration limiting and reminder messages for the codebase search agent.
struct IterationLimiter {
    max_iterations: usize,
}

impl IterationLimiter {
    fn new(max_iterations: usize) -> Self {
        Self { max_iterations }
    }

    /// Applies a reminder message to the conversation if needed based on the current request count.
    fn apply_reminder_if_needed(
        &self,
        request_count: usize,
        conversation: &mut Conversation,
    ) {
        let Some(ctx) = conversation.context.take() else {
            return;
        };

        let remaining = self.max_iterations.saturating_sub(request_count);
        let halfway = self.max_iterations / 2;
        let urgent_threshold = self.max_iterations.saturating_sub(2);

        let tool = forge_domain::ToolName::new("report_search")
        let (message, force_tool) = match request_count {
            0 => return,
            n if n == halfway => (
                format!(
                    "<system-reminder>You have used {n} of {} requests. \
                     You have {remaining} requests remaining before you must call \
                {} to report your findings.</system-reminder>",
                    self.max_iterations,
                    tool.as_str(),
                ),
                false,
            ),
            n if n >= urgent_threshold && n < self.max_iterations => (
                format!(
                    "<system-reminder>URGENT: You have used {n} of {} requests. \
                     Only {remaining} request(s) remaining! You MUST call {} on your \
                     next turn to report your findings.</system-reminder>",
                    self.max_iterations,
                    tool.as_str()
                ),
                false,
            ),
            n if n == self.max_iterations + 1 => (
                format!("<system-reminder>FINAL REMINDER: You have reached the maximum number of requests. \
                 You MUST call the {} tool now to report your findings. \
                 Do not make any more search requests.</system-reminder>", tool.as_str()
            ),
                true,
            ),
            _ => return,
        };

        let text_msg = TextMessage::new(Role::User, message);
        conversation.context = Some(if force_tool {
            ctx.add_message(ContextMessage::Text(text_msg)).tool_choice(
                forge_domain::ToolChoice::Call(tool),
            )
        } else {
            ctx.add_message(ContextMessage::Text(text_msg))
        });
    }
}

/// Creates a hook for the codebase search agent with iteration limiting and output capture.
///
/// This hook only applies its behavior when used with the codebase_search agent.
/// For other agents, it operates as a no-op.
fn codebase_search_hook(
    max_iterations: usize,
    captured_output: Arc<Mutex<Option<ToolResult>>>,
) -> Hook {
    let limiter = IterationLimiter::new(max_iterations);

    Hook::default()
        .on_request({
            move |event: &EventData<RequestPayload>, conversation: &mut Conversation| {
                // Only apply iteration limiting for codebase_search agent
                if event.agent.id.is_codebase_search() {
                    limiter.apply_reminder_if_needed(event.payload.request_count, conversation);
                }
                async move { Ok(()) }
            }
        })
        .on_toolcall_end({
            move |event: &EventData<ToolcallEndPayload>, _conversation: &mut Conversation| {
                let captured_output = captured_output.clone();
                let agent_id = event.agent.id.clone();
                let result = event.payload.result.clone();
                async move {
                    // Only capture report_search for codebase_search agent
                    if agent_id.is_codebase_search() {
                        if result.name.as_str() == tool{
                            *captured_output.lock().await = Some(result);
                        }
                    }
                    Ok(())
                }
            }
        })
}


#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::{Agent, ModelId, ProviderId, ToolOutput};

    #[test]
    fn test_halfway_reminder() {
        let limiter = IterationLimiter::new(10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        limiter.apply_reminder_if_needed(5, &mut conversation);

        let ctx = conversation.context.as_ref().unwrap();
        assert_eq!(ctx.messages.len(), 1);
        let msg = ctx.messages[0].to_text();
        assert!(msg.contains("You have used 5 of 10 requests"));
        assert!(msg.contains("5 requests remaining"));
        assert!(ctx.tool_choice.is_none());
    }

    #[test]
    fn test_urgent_reminder() {
        let limiter = IterationLimiter::new(10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        limiter.apply_reminder_if_needed(8, &mut conversation);

        let ctx = conversation.context.as_ref().unwrap();
        let msg = ctx.messages[0].to_text();
        assert!(msg.contains("URGENT"));
        assert!(msg.contains("You have used 8 of 10 requests"));
        assert!(msg.contains("2 request(s) remaining"));
        assert!(ctx.tool_choice.is_none());
    }

    #[test]
    fn test_final_reminder_forces_tool_choice() {
        let limiter = IterationLimiter::new(10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        limiter.apply_reminder_if_needed(11, &mut conversation);

        let ctx = conversation.context.as_ref().unwrap();
        let msg = ctx.messages[0].to_text();
        assert!(msg.contains("FINAL REMINDER"));
        assert!(msg.contains("reached the maximum number of requests"));
        assert!(matches!(
            ctx.tool_choice.as_ref().unwrap(),
            forge_domain::ToolChoice::Call(name) if name.as_str() == "report_search"
        ));
    }

    #[test]
    fn test_no_reminder_without_context() {
        let limiter = IterationLimiter::new(10);
        let mut conversation = Conversation::generate().title(Some("test".to_string()));

        limiter.apply_reminder_if_needed(5, &mut conversation);

        assert!(conversation.context.is_none());
    }

    #[test]
    fn test_reminder_preserves_existing_messages() {
        let limiter = IterationLimiter::new(10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Add an existing message
        conversation.context = Some(
            conversation.context.unwrap()
                .add_message(ContextMessage::Text(TextMessage::new(
                    Role::User,
                    "existing message".to_string(),
                )))
        );

        limiter.apply_reminder_if_needed(5, &mut conversation);

        let ctx = conversation.context.as_ref().unwrap();
        assert_eq!(ctx.messages.len(), 2);
        assert!(ctx.messages[0].to_text().contains("existing message"));
        assert!(ctx.messages[1].to_text().contains("You have used 5"));
    }

    #[tokio::test]
    async fn test_codebase_search_hook_request_handler_applies_limiting() {
        use forge_domain::EventHandle;

        let captured_output = Arc::new(Mutex::new(None));
        let hook = codebase_search_hook(10, captured_output);

        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Simulate request from codebase_search agent at halfway point
        hook
            .handle(
                LifecycleEvent::Request {
                    agent: Agent::new(
                        AgentId::new("codebase_search"),
                        ProviderId::FORGE,
                        ModelId::new("test-model"),
                    ),
                    model_id: ModelId::new("test-model"),
                    request_count: 5,
                },
                &mut conversation,
            )
            .await
            .unwrap();

        // Verify reminder was added
        let ctx = conversation.context.as_ref().unwrap();
        assert_eq!(ctx.messages.len(), 1);
        assert!(ctx.messages[0].to_text().contains("You have used 5 of 10 requests"));
    }

    #[tokio::test]
    async fn test_codebase_search_hook_captures_report_search() {
        use forge_domain::EventHandle;

        let captured_output = Arc::new(Mutex::new(None));
        let hook = codebase_search_hook(10, captured_output.clone());

        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Create a mock ToolResult
        let result = ToolResult {
            call_id: Some(forge_domain::ToolCallId::new("test_call")),
            name: ToolName::new("report_search"),
            output: ToolOutput::text("Found 3 files"),
        };

        // Simulate toolcall_end event
        hook
            .handle(LifecycleEvent::ToolcallEnd(result), &mut conversation)
            .await
            .unwrap();

        // Verify output was captured
        let captured = captured_output.lock().await.take();
        assert!(captured.is_some());
        assert_eq!(captured.unwrap().name.as_str(), "report_search");
    }
}