use std::fmt::Write;

use async_trait::async_trait;
use forge_domain::{
    ContextMessage, Conversation, EventData, EventHandle, FinishReason, ResponsePayload, TodoStatus,
};

/// Detects when the LLM signals task completion while there are still
/// pending or in-progress todo items.
///
/// When triggered, it injects a formatted reminder listing all
/// outstanding todos into the conversation context, preventing the
/// orchestrator from yielding prematurely.
#[derive(Debug, Clone, Default)]
pub struct PendingTodosHandler;

impl PendingTodosHandler {
    /// Creates a new pending-todos handler
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EventHandle<EventData<ResponsePayload>> for PendingTodosHandler {
    async fn handle(
        &self,
        event: &EventData<ResponsePayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let message = &event.payload.message;

        // Only act when the model signals completion (stop + no tool calls)
        let is_complete =
            message.finish_reason == Some(FinishReason::Stop) && message.tool_calls.is_empty();

        if !is_complete {
            return Ok(());
        }

        let pending_todos = conversation.metrics.get_active_todos();
        if pending_todos.is_empty() {
            return Ok(());
        }

        let mut reminder = String::from(
            "You have pending todo items that must be completed before finishing the task:\n\n",
        );
        for todo in &pending_todos {
            let status = match todo.status {
                TodoStatus::Pending => "PENDING",
                TodoStatus::InProgress => "IN_PROGRESS",
                _ => continue,
            };
            writeln!(reminder, "- [{}] {}", status, todo.content)
                .expect("Writing to String should not fail");
        }
        writeln!(
            reminder,
            "\nPlease complete all pending items before finishing."
        )
        .expect("Writing to String should not fail");

        if let Some(context) = conversation.context.as_mut() {
            context
                .messages
                .push(ContextMessage::user(reminder, None).into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Agent, ChatCompletionMessageFull, Context, Conversation, EventData, EventHandle,
        FinishReason, Metrics, ModelId, ResponsePayload, Todo, TodoStatus, ToolCallFull, ToolName,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_agent() -> Agent {
        Agent::new(
            "test-agent",
            "test-provider".to_string().into(),
            ModelId::new("test-model"),
        )
    }

    fn fixture_conversation(todos: Vec<Todo>) -> Conversation {
        let mut conversation = Conversation::generate();
        conversation.context = Some(Context::default());
        conversation.metrics = Metrics::default().todos(todos);
        conversation
    }

    fn fixture_event(
        finish_reason: Option<FinishReason>,
        has_tool_calls: bool,
    ) -> EventData<ResponsePayload> {
        let mut message = ChatCompletionMessageFull {
            content: String::new(),
            thought_signature: None,
            reasoning: None,
            reasoning_details: None,
            tool_calls: vec![],
            usage: Default::default(),
            finish_reason,
            phase: None,
        };
        if has_tool_calls {
            message.tool_calls = vec![ToolCallFull::new(ToolName::from("test-tool"))];
        }
        EventData::new(
            fixture_agent(),
            ModelId::new("test-model"),
            ResponsePayload::new(message),
        )
    }

    #[tokio::test]
    async fn test_no_pending_todos_does_nothing() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event(Some(FinishReason::Stop), false);
        let mut conversation = fixture_conversation(vec![]);

        let initial_msg_count = conversation.context.as_ref().unwrap().messages.len();
        handler.handle(&event, &mut conversation).await.unwrap();

        let actual = conversation.context.as_ref().unwrap().messages.len();
        let expected = initial_msg_count;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_pending_todos_injects_reminder() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event(Some(FinishReason::Stop), false);
        let mut conversation = fixture_conversation(vec![
            Todo::new("Fix the build").status(TodoStatus::Pending),
            Todo::new("Write tests").status(TodoStatus::InProgress),
        ]);

        handler.handle(&event, &mut conversation).await.unwrap();

        let actual = conversation.context.as_ref().unwrap().messages.len();
        let expected = 1;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_reminder_contains_formatted_list() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event(Some(FinishReason::Stop), false);
        let mut conversation = fixture_conversation(vec![
            Todo::new("Fix the build").status(TodoStatus::Pending),
            Todo::new("Write tests").status(TodoStatus::InProgress),
        ]);

        handler.handle(&event, &mut conversation).await.unwrap();

        let entry = &conversation.context.as_ref().unwrap().messages[0];
        let actual = entry.message.content().unwrap();
        assert!(actual.contains("- [PENDING] Fix the build"));
        assert!(actual.contains("- [IN_PROGRESS] Write tests"));
    }

    #[tokio::test]
    async fn test_tool_calls_present_does_not_trigger() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event(Some(FinishReason::Stop), true);
        let mut conversation =
            fixture_conversation(vec![Todo::new("Fix the build").status(TodoStatus::Pending)]);

        let initial_msg_count = conversation.context.as_ref().unwrap().messages.len();
        handler.handle(&event, &mut conversation).await.unwrap();

        let actual = conversation.context.as_ref().unwrap().messages.len();
        let expected = initial_msg_count;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_non_stop_finish_reason_does_not_trigger() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event(Some(FinishReason::Length), false);
        let mut conversation =
            fixture_conversation(vec![Todo::new("Fix the build").status(TodoStatus::Pending)]);

        let initial_msg_count = conversation.context.as_ref().unwrap().messages.len();
        handler.handle(&event, &mut conversation).await.unwrap();

        let actual = conversation.context.as_ref().unwrap().messages.len();
        let expected = initial_msg_count;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_completed_todos_not_included() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event(Some(FinishReason::Stop), false);
        let mut conversation = fixture_conversation(vec![
            Todo::new("Completed task").status(TodoStatus::Completed),
            Todo::new("Cancelled task").status(TodoStatus::Cancelled),
        ]);

        let initial_msg_count = conversation.context.as_ref().unwrap().messages.len();
        handler.handle(&event, &mut conversation).await.unwrap();

        let actual = conversation.context.as_ref().unwrap().messages.len();
        let expected = initial_msg_count;
        assert_eq!(actual, expected);
    }
}
