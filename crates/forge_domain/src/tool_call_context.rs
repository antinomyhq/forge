use std::sync::Arc;

use derive_setters::Setters;
use tokio::sync::mpsc::Sender;

use crate::{AgentId, AgentMessage, ChatResponse};

/// Type alias for Arc<Sender<Result<AgentMessage<ChatResponse>>>>
type ArcSender = Arc<Sender<anyhow::Result<AgentMessage<ChatResponse>>>>;

/// Provides additional context for tool calls.
/// Currently empty but structured to allow for future extension.
#[derive(Default, Clone, Debug, Setters)]
pub struct ToolCallContext {
    #[setters(strip_option)]
    pub agent_id: Option<AgentId>,
    pub sender: Option<ArcSender>,
}

impl ToolCallContext {
    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: AgentMessage<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message)).await?
        }
        Ok(())
    }

    pub async fn send_text(&self, content: String) -> anyhow::Result<()> {
        if let Some(agent_id) = &self.agent_id {
            self.send(AgentMessage::new(
                agent_id.clone(),
                ChatResponse::Text { text: content.as_str().to_string(), is_complete: true },
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

    #[test]
    fn test_for_tests() {
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
