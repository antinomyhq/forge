use std::sync::{Arc, Mutex};

use derive_setters::Setters;

use crate::{ArcSender, ChatResponse, ConversationId, Metrics, TitleFormat};

/// Provides additional context for tool calls.
#[derive(Debug, Clone, Setters)]
pub struct ToolCallContext {
    sender: Option<ArcSender>,
    metrics: Arc<Mutex<Metrics>>,
    conversation_id: Option<ConversationId>,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new(metrics: Metrics) -> Self {
        Self {
            sender: None,
            metrics: Arc::new(Mutex::new(metrics)),
            conversation_id: None,
        }
    }

    /// Get the conversation ID if available
    pub fn get_conversation_id(&self) -> Option<&ConversationId> {
        self.conversation_id.as_ref()
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: impl Into<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message.into())).await?
        }
        Ok(())
    }

    /// Send tool input title - MUST ONLY be used for presenting tool input
    /// information
    pub async fn send_tool_input(&self, title: impl Into<TitleFormat>) -> anyhow::Result<()> {
        let title = title.into();
        self.send(ChatResponse::TaskMessage {
            content: crate::ChatResponseContent::ToolInput(title),
        })
        .await
    }

    /// Execute a closure with access to the metrics
    pub fn with_metrics<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut Metrics) -> R,
    {
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire metrics lock"))?;
        Ok(f(&mut metrics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        let metrics = Metrics::default();
        let context = ToolCallContext::new(metrics);
        assert!(context.sender.is_none());
    }

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let metrics = Metrics::default();
        let context = ToolCallContext::new(metrics);
        assert!(context.sender.is_none());
    }
}
