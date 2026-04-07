use async_trait::async_trait;
use forge_domain::{
    ContextMessage, Conversation, EndPayload, EventData, EventHandle, Template, TodoStatus,
};
use forge_template::Element;
use serde::Serialize;

use crate::TemplateEngine;

/// A single todo item prepared for template rendering.
#[derive(Serialize)]
struct TodoReminderItem {
    status: &'static str,
    content: String,
}

/// Template context for the pending-todos reminder.
#[derive(Serialize)]
struct PendingTodosContext {
    todos: Vec<TodoReminderItem>,
}

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
impl EventHandle<EventData<EndPayload>> for PendingTodosHandler {
    async fn handle(
        &self,
        _event: &EventData<EndPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let pending_todos = conversation.metrics.get_active_todos();
        if pending_todos.is_empty() {
            return Ok(());
        }

        // Check if a reminder was already added to avoid duplicates
        if let Some(context) = &conversation.context {
            let has_existing_reminder = context.messages.iter().any(|entry| {
                entry
                    .message
                    .content()
                    .map_or(false, |content| content.contains("pending todo items"))
            });
            if has_existing_reminder {
                return Ok(());
            }
        }

        let todo_items: Vec<TodoReminderItem> = pending_todos
            .iter()
            .filter_map(|todo| {
                let status = match todo.status {
                    TodoStatus::Pending => "PENDING",
                    TodoStatus::InProgress => "IN_PROGRESS",
                    _ => return None,
                };
                Some(TodoReminderItem { status, content: todo.content.clone() })
            })
            .collect();

        let ctx = PendingTodosContext { todos: todo_items };
        let reminder = TemplateEngine::default().render(
            Template::<PendingTodosContext>::new("forge-pending-todos-reminder.md"),
            &ctx,
        )?;

        if let Some(context) = conversation.context.as_mut() {
            let content = Element::new("system_reminder").cdata(reminder);
            context
                .messages
                .push(ContextMessage::user(content, None).into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Agent, Context, Conversation, EndPayload, EventData, EventHandle, Metrics, ModelId, Todo,
        TodoStatus,
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

    fn fixture_event() -> EventData<EndPayload> {
        EventData::new(fixture_agent(), ModelId::new("test-model"), EndPayload)
    }

    #[tokio::test]
    async fn test_no_pending_todos_does_nothing() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event();
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
        let event = fixture_event();
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
        let event = fixture_event();
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
    async fn test_completed_todos_not_included() {
        let handler = PendingTodosHandler::new();
        let event = fixture_event();
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
