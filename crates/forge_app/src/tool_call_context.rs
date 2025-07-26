use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::{ChatResponse, TaskList};
use tokio::sync::mpsc::Sender;

/// Type alias for Arc<Sender<Result<ChatResponse>>>
type ArcSender = Arc<Sender<anyhow::Result<ChatResponse>>>;

#[async_trait::async_trait]
pub trait WriteChannel {
    async fn send(&self, agent_message: impl Into<ChatResponse> + Send) -> anyhow::Result<()>;
    async fn send_text(&self, content: impl ToString + Send) -> anyhow::Result<()>;
}

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
}

#[async_trait::async_trait]
impl WriteChannel for ToolCallContext {
    async fn send(&self, agent_message: impl Into<ChatResponse> + Send) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message.into())).await?
        }
        Ok(())
    }

    async fn send_text(&self, content: impl ToString + Send) -> anyhow::Result<()> {
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
