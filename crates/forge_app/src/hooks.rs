use std::sync::Arc;

use forge_domain::{
    AgentId, ContextMessage, Conversation, EventData, Hook, RequestPayload, Role, TextMessage,
    ToolName, ToolResult, ToolcallEndPayload,
};
use tokio::sync::Mutex;

/// Manages tool call reminders for agents.
///
/// Sends reminder messages at strategic points to ensure the agent
/// calls the specified tool before exceeding the iteration limit.
struct ToolCallReminder {
    tool_name: ToolName,
    max_iterations: usize,
}

impl ToolCallReminder {
    fn new(tool_name: ToolName, max_iterations: usize) -> Self {
        Self { tool_name, max_iterations }
    }

    /// Applies a reminder message to the conversation if needed based on the
    /// current request count.
    ///
    /// Reminders are sent at:
    /// - Halfway point: Informational reminder
    /// - Urgent threshold (max - 2): Urgent warning
    /// - Final (max + 1): Forces the tool call
    fn apply(&self, request_count: usize, conversation: &mut Conversation) {
        let Some(ctx) = conversation.context.take() else {
            return;
        };

        let remaining = self.max_iterations.saturating_sub(request_count);
        let halfway = self.max_iterations / 2;
        let urgent_threshold = self.max_iterations.saturating_sub(2);

        let (message, force_tool) = match request_count {
            0 => return,
            n if n == halfway => (
                format!(
                    "<system-reminder>You have used {n} of {} requests. \
                     You have {remaining} requests remaining before you must call \
{} to report your findings.</system-reminder>",
                    self.max_iterations,
                    self.tool_name.as_str(),
                ),
                false,
            ),
            n if n >= urgent_threshold && n < self.max_iterations => (
                format!(
                    "<system-reminder>URGENT: You have used {n} of {} requests. \
                     Only {remaining} request(s) remaining! You MUST call {} on your \
                     next turn to report your findings.</system-reminder>",
                    self.max_iterations,
                    self.tool_name.as_str()
                ),
                false,
            ),
            n if n == self.max_iterations + 1 => (
                format!(
                    "<system-reminder>FINAL REMINDER: You have reached the maximum number of requests. \
                 You MUST call the {} tool now to report your findings. \
                 Do not make any more search requests.</system-reminder>",
                    self.tool_name.as_str()
                ),
                true,
            ),
            _ => return,
        };

        let text_msg = TextMessage::new(Role::User, message);
        conversation.context = Some(if force_tool {
            ctx.add_message(ContextMessage::Text(text_msg))
                .tool_choice(forge_domain::ToolChoice::Call(self.tool_name.clone()))
        } else {
            ctx.add_message(ContextMessage::Text(text_msg))
        });
    }
}

/// Creates a hook that reminds the agent to call a tool before exceeding
/// iteration limits.
///
/// Adds reminder messages at key milestones (halfway, urgent, final) and forces
/// the tool call when max iterations is reached.
pub fn tool_call_reminder(agent_id: AgentId, tool_name: ToolName, max_iterations: usize) -> Hook {
    let reminder = ToolCallReminder::new(tool_name, max_iterations);
    Hook::default().on_request({
        move |event: &EventData<RequestPayload>, conversation: &mut Conversation| {
            if event.agent.id == agent_id {
                reminder.apply(event.payload.request_count, conversation);
            }
            async move { Ok(()) }
        }
    })
}

/// Creates a hook that captures the output of a tool call.
///
/// Stores the tool result in the provided shared storage when the specified
/// tool is called by the specified agent.
pub fn tool_output_capture(
    agent_id: AgentId,
    tool_name: ToolName,
    captured_output: Arc<Mutex<Option<ToolResult>>>,
) -> Hook {
    Hook::default().on_toolcall_end({
        move |event: &EventData<ToolcallEndPayload>, _conversation: &mut Conversation| {
            let captured_output = captured_output.clone();
            let event_agent_id = event.agent.id.clone();
            let result = event.payload.result.clone();
            let expected_agent = agent_id.clone();
            let expected_tool = tool_name.clone();
            async move {
                if event_agent_id == expected_agent && result.name == expected_tool {
                    *captured_output.lock().await = Some(result);
                }
                Ok(())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Agent, AgentId, EventHandle, LifecycleEvent, ModelId, ProviderId, RequestPayload, ToolName,
        ToolOutput, ToolcallEndPayload,
    };

    use super::*;

    // Tests for ToolCallReminder
    #[test]
    fn test_halfway_reminder() {
        let tool_name = ToolName::new("report_search");
        let reminder = ToolCallReminder::new(tool_name, 10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        reminder.apply(5, &mut conversation);

        insta::assert_snapshot!(conversation.context.unwrap().to_text());
    }

    #[test]
    fn test_urgent_reminder() {
        let tool_name = ToolName::new("report_search");
        let reminder = ToolCallReminder::new(tool_name, 10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        reminder.apply(8, &mut conversation);

        insta::assert_snapshot!(conversation.context.unwrap().to_text());
    }

    #[test]
    fn test_final_reminder_forces_tool_choice() {
        let tool_name = ToolName::new("report_search");
        let reminder = ToolCallReminder::new(tool_name, 10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        reminder.apply(11, &mut conversation);

        insta::assert_snapshot!(conversation.context.unwrap().to_text());
    }

    #[test]
    fn test_no_reminder_without_context() {
        let tool_name = ToolName::new("report_search");
        let reminder = ToolCallReminder::new(tool_name, 10);
        let mut conversation = Conversation::generate().title(Some("test".to_string()));

        reminder.apply(5, &mut conversation);

        assert!(conversation.context.is_none());
    }

    #[test]
    fn test_reminder_preserves_existing_messages() {
        let tool_name = ToolName::new("report_search");
        let reminder = ToolCallReminder::new(tool_name, 10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Add an existing message
        conversation.context = Some(conversation.context.unwrap().add_message(
            ContextMessage::Text(TextMessage::new(Role::User, "existing message".to_string())),
        ));

        reminder.apply(5, &mut conversation);

        insta::assert_snapshot!(conversation.context.unwrap().to_text());
    }

    // Tests for hooks

    #[tokio::test]
    async fn test_tool_call_reminder() {
        let agent_id = AgentId::new("codebase_search");
        let tool_name = ToolName::new("report_search");
        let hook = tool_call_reminder(agent_id.clone(), tool_name, 10);

        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Simulate request from codebase_search agent at halfway point
        let agent = Agent::new(agent_id, ProviderId::FORGE, ModelId::new("test-model"));
        let event = LifecycleEvent::Request(EventData::new(
            agent,
            ModelId::new("test-model"),
            RequestPayload::new(5),
        ));
        hook.handle(&event, &mut conversation).await.unwrap();

        // Verify reminder was added
        let ctx = conversation.context.as_ref().unwrap();
        assert_eq!(ctx.messages.len(), 1);
        assert!(
            ctx.messages[0]
                .to_text()
                .contains("You have used 5 of 10 requests")
        );
    }

    #[tokio::test]
    async fn test_tool_output_capture() {
        let agent_id = AgentId::new("codebase_search");
        let tool_name = ToolName::new("report_search");
        let captured_output = Arc::new(Mutex::new(None));
        let hook =
            tool_output_capture(agent_id.clone(), tool_name.clone(), captured_output.clone());

        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Create a mock ToolResult
        let result = ToolResult {
            call_id: Some(forge_domain::ToolCallId::new("test_call")),
            name: tool_name,
            output: ToolOutput::text("Found 3 files"),
        };

        // Simulate toolcall_end event
        let agent = Agent::new(agent_id, ProviderId::FORGE, ModelId::new("test-model"));
        let event = LifecycleEvent::ToolcallEnd(EventData::new(
            agent,
            ModelId::new("test-model"),
            ToolcallEndPayload::new(result.clone()),
        ));
        hook.handle(&event, &mut conversation).await.unwrap();

        // Verify output was captured
        let captured = captured_output.lock().await.take();
        assert!(captured.is_some());
        assert_eq!(captured.unwrap().name.as_str(), "report_search");
    }
}
