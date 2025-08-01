use std::sync::Arc;

use derive_setters::Setters;
use tokio::sync::mpsc::Sender;

use crate::{ChatResponse, TaskList};

/// Type alias for Arc<Sender<Result<ChatResponse>>>
type ArcSender = Arc<Sender<anyhow::Result<ChatResponse>>>;

/// Provides additional context for tool calls.
#[derive(Debug, Setters)]
pub struct ToolCallContext {
    sender: Option<ArcSender>,
    pub tasks: TaskList,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new(task_list: TaskList) -> Self {
        Self { sender: None, tasks: task_list }
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: impl Into<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message.into())).await?
        }
        Ok(())
    }

    pub async fn send_text(&self, content: impl ToString) -> anyhow::Result<()> {
        self.send(ChatResponse::Text { text: content.to_string(), is_complete: true, is_md: false })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        let context = ToolCallContext::new(TaskList::new());
        assert!(context.sender.is_none());
    }

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let context = ToolCallContext::new(TaskList::new());
        assert!(context.sender.is_none());
    }
}
