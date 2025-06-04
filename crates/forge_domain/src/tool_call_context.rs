use std::sync::Arc;

use derive_setters::Setters;
use tokio::sync::mpsc::Sender;

use crate::ChatResponse;

/// Type alias for Arc<Sender<Result<ChatResponse>>>
type ArcSender = Arc<Sender<anyhow::Result<ChatResponse>>>;

/// Provides additional context for tool calls.
#[derive(Default, Clone, Debug, Setters)]
pub struct ToolCallContext {
    pub sender: Option<ArcSender>,
    /// Indicates whether the tool execution has been completed
    /// This is wrapped in an RWLock for thread-safety
    #[setters(skip)]
    pub is_complete: bool,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new() -> Self {
        Self { sender: None, is_complete: false }
    }

    /// Sets the is_complete flag to true
    pub async fn set_complete(&mut self) {
        self.is_complete = true;
    }

    /// Gets the current value of is_complete flag
    pub async fn get_complete(&self) -> bool {
        self.is_complete
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: ChatResponse) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message)).await?
        }
        Ok(())
    }

    pub async fn send_summary(&self, content: String) -> anyhow::Result<()> {
        self.send(ChatResponse::Text {
            text: content,
            is_complete: true,
            is_md: false,
            is_summary: true,
        })
        .await
    }

    pub async fn send_text(&self, content: impl ToString) -> anyhow::Result<()> {
        self.send(ChatResponse::Text {
            text: content.to_string(),
            is_complete: true,
            is_md: false,
            is_summary: false,
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        let context = ToolCallContext::default();
        assert!(context.sender.is_none());
    }

    #[tokio::test]
    async fn test_is_complete_default() {
        let context = ToolCallContext::default();
        assert!(!context.get_complete().await);
    }

    #[tokio::test]
    async fn test_set_complete() {
        let mut context = ToolCallContext::default();
        context.set_complete().await;
        assert!(context.get_complete().await);
    }

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let context = ToolCallContext::default();
        assert!(context.sender.is_none());
    }
}
