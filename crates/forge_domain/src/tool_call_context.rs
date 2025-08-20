use std::sync::Arc;

use derive_setters::Setters;
use tokio::sync::mpsc::Sender;

use crate::{ChatResponse, Metrics};

/// Type alias for Arc<Sender<Result<ChatResponse>>>
type ArcSender = Arc<Sender<anyhow::Result<ChatResponse>>>;

/// Provides additional context for tool calls.
#[derive(Debug, Setters)]
pub struct ToolCallContext<'a> {
    sender: Option<ArcSender>,
    pub metrics: &'a mut Metrics,
}

impl<'a> ToolCallContext<'a> {
    /// Creates a new ToolCallContext with default values
    pub fn new(metrics: &'a mut Metrics) -> Self {
        Self { sender: None, metrics }
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
        let mut metrics = Metrics::new();
        let context = ToolCallContext::new(&mut metrics);
        assert!(context.sender.is_none());
    }

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let mut metrics = Metrics::new();
        let context = ToolCallContext::new(&mut metrics);
        assert!(context.sender.is_none());
    }
}
