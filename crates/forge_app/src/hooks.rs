use forge_domain::{
    AgentId, ContextMessage, Conversation, EventData, Exit, Hook, RequestPayload, Role,
    TextMessage, ToolName, ToolcallEndPayload,
};
use forge_template::Element;

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
    ///
    /// Does not add a reminder if the target tool is already being called in
    /// the current iteration.
    fn apply(&self, request_count: usize, conversation: &mut Conversation) {
        let Some(ctx) = conversation.context.take() else {
            return;
        };

        // Check if the target tool was called or already executed in the last message
        if let Some(last_entry) = ctx.messages.last() {
            match &last_entry.message {
                ContextMessage::Text(text_msg) => {
                    if let Some(tool_calls) = &text_msg.tool_calls
                        && tool_calls.iter().any(|call| call.name == self.tool_name)
                    {
                        // Tool is already being called, don't add reminder
                        conversation.context = Some(ctx);
                        return;
                    }
                }
                ContextMessage::Tool(tool_result) => {
                    if tool_result.name == self.tool_name {
                        // Tool is already being called, don't add reminder
                        conversation.context = Some(ctx);
                        return;
                    }
                }
                ContextMessage::Image(_) => {}
            }
        }

        let remaining = self.max_iterations.saturating_sub(request_count);
        let halfway = self.max_iterations / 2;
        let urgent_threshold = self.max_iterations.saturating_sub(2);

        let (message, force_tool) = match request_count {
            0 => {
                conversation.context = Some(ctx);
                return;
            }
            n if n == halfway => (
                Element::new("system-reminder")
                    .text(format!(
                        "You have used {n} of {} requests. \
                         You have {remaining} requests remaining before you must call \
{} to report your findings.",
                        self.max_iterations,
                        self.tool_name.as_str(),
                    ))
                    .render(),
                false,
            ),
            n if n >= urgent_threshold && n < self.max_iterations => (
                Element::new("system-reminder")
                    .text(format!(
                        "URGENT: You have used {n} of {} requests. \
                         Only {remaining} request(s) remaining! You MUST call {} on your \
                         next turn to report your findings.",
                        self.max_iterations,
                        self.tool_name.as_str()
                    ))
                    .render(),
                false,
            ),
            n if n == self.max_iterations + 1 => (
                Element::new("system-reminder")
                    .text(format!(
                        "FINAL REMINDER: You have reached the maximum number of requests. \
                     You MUST call the {} tool now to report your findings. \
                     Do not make any more search requests.",
                        self.tool_name.as_str()
                    ))
                    .render(),
                true,
            ),
            _ => {
                conversation.context = Some(ctx);
                return;
            }
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
            async { None }
        }
    })
}

/// Creates a hook that captures the output of a tool call.
///
/// Returns Exit with the captured tool result when the specified tool is called
/// by the specified agent.
pub fn tool_output_capture(agent_id: AgentId, tool_name: ToolName) -> Hook {
    Hook::default().on_toolcall_end({
        move |event: &EventData<ToolcallEndPayload>, conversation: &mut Conversation| {
            let event_agent_id = event.agent.id.clone();
            let result = event.payload.result.clone();
            let expected_agent = agent_id.clone();
            let expected_tool = tool_name.clone();
            let conversation_id = conversation.id;
            async move {
                if event_agent_id == expected_agent && result.name == expected_tool {
                    Some(Exit::tool(result, conversation_id))
                } else {
                    None
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Agent, AgentId, EventHandle, LifecycleEvent, ModelId, ProviderId, RequestPayload, ToolName,
        ToolOutput, ToolResult, ToolcallEndPayload,
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

    #[test]
    fn test_no_reminder_when_tool_already_called() {
        let tool_name = ToolName::new("report_search");
        let reminder = ToolCallReminder::new(tool_name.clone(), 10);
        let mut conversation = Conversation::generate()
            .title(Some("test".to_string()))
            .context(forge_domain::Context::default());

        // Add a message with a tool call to the target tool
        let mut text_msg = TextMessage::new(Role::Assistant, "I'll search for you.".to_string());
        text_msg.tool_calls = Some(vec![forge_domain::ToolCallFull {
            name: tool_name,
            call_id: Some(forge_domain::ToolCallId::new("test_call")),
            arguments: forge_domain::ToolCallArguments::from_json(r#"{"tasks": ["search task"]}"#),
        }]);

        conversation.context = Some(
            conversation
                .context
                .unwrap()
                .add_message(ContextMessage::Text(text_msg)),
        );

        // Apply reminder at halfway point - should not add reminder
        reminder.apply(5, &mut conversation);

        // Verify no reminder was added (should still have only 1 message)
        let ctx = conversation.context.as_ref().unwrap();
        assert_eq!(ctx.messages.len(), 1);
        assert!(!ctx.messages[0].to_text().contains("You have used"));
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
        hook.handle(&event, &mut conversation).await;

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
        let hook = tool_output_capture(agent_id.clone(), tool_name.clone());

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
        let exit = hook.handle(&event, &mut conversation).await;

        // Verify Exit was returned with the tool result
        assert!(exit.is_some());
        let exit = exit.unwrap();
        let captured_result = exit.as_tool_result().unwrap();
        assert_eq!(captured_result.name.as_str(), "report_search");
    }
}
