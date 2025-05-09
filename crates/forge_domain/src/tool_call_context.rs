use std::sync::Arc;

use derive_setters::Setters;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::{AgentId, AgentMessage, ChatResponse};

/// Type alias for Arc<Sender<Result<AgentMessage<ChatResponse>>>>
type ArcSender = Arc<Sender<anyhow::Result<AgentMessage<ChatResponse>>>>;

/// Provides additional context for tool calls.
#[derive(Default, Clone, Debug, Setters)]
pub struct ToolCallContext {
    #[setters(strip_option)]
    pub agent_id: Option<AgentId>,
    pub sender: Option<ArcSender>,
    /// Indicates whether the tool execution has been completed
    /// This is wrapped in an RWLock for thread-safety
    #[setters(skip)]
    pub is_complete: Arc<RwLock<bool>>,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new() -> Self {
        Self {
            agent_id: None,
            sender: None,
            is_complete: Arc::new(RwLock::new(false)),
        }
    }

    /// Sets the is_complete flag to true
    pub async fn set_complete(&self) {
        let mut is_complete = self.is_complete.write().await;
        *is_complete = true;
    }

    /// Gets the current value of is_complete flag
    pub async fn get_complete(&self) -> bool {
        *self.is_complete.read().await
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: AgentMessage<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message)).await?
        }
        Ok(())
    }

    pub async fn send_summary(&self, content: String) -> anyhow::Result<()> {
        if let Some(agent_id) = &self.agent_id {
            self.send(AgentMessage::new(
                agent_id.clone(),
                ChatResponse::Text {
                    text: content.as_str().to_string(),
                    is_complete: true,
                    is_md: false,
                    is_summary: true,
                },
            ))
            .await
        } else {
            Ok(())
        }
    }

    pub async fn send_text(&self, content: impl ToString) -> anyhow::Result<()> {
        if let Some(agent_id) = &self.agent_id {
            self.send(AgentMessage::new(
                agent_id.clone(),
                ChatResponse::Text {
                    text: content.to_string(),
                    is_complete: true,
                    is_md: false,
                    is_summary: false,
                },
            ))
            .await
        } else {
            Ok(())
        }
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
        let context = ToolCallContext::default();
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
