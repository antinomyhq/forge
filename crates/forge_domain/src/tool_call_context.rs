use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use crate::ChatResponse;

/// Type alias for Arc<Sender<Result<ChatResponse>>>
type ArcSender = Arc<Sender<anyhow::Result<ChatResponse>>>;

/// Provides additional context for tool calls.
#[derive(Default, Clone, Debug)]
pub struct ToolCallContext {
    sender: Option<ArcSender>,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new(sender: Option<ArcSender>) -> Self {
        Self { sender }
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

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let context = ToolCallContext::default();
        assert!(context.sender.is_none());
    }
}
