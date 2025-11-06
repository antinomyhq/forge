use std::sync::{Arc, Mutex};

use anyhow::Context;
use derive_setters::Setters;

use crate::{ArcSender, ChatResponse, ChatResponseContent, Metrics, TitleFormat, Usage};

/// Provides additional context for tool calls.
#[derive(Debug, Clone, Setters)]
pub struct ToolCallContext {
    sender: Option<ArcSender>,
    metrics: Arc<Mutex<Metrics>>,
    usage: Arc<Mutex<Option<Usage>>>,
    token_limit: Option<usize>,
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new(metrics: Metrics) -> Self {
        Self {
            sender: None,
            metrics: Arc::new(Mutex::new(metrics)),
            usage: Arc::new(Mutex::new(None)),
            token_limit: None,
        }
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: impl Into<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            let agent_message = match agent_message.into() {
                ChatResponse::TaskMessage {
                    content: ChatResponseContent::Title(mut title_format),
                } => {
                    // Attach current usage to the title if available
                    if let Ok(usage_lock) = self.usage.lock()
                        && let Some(usage) = usage_lock.as_ref()
                    {
                        title_format.usage = Some(usage.clone());
                    }

                    // Attach token limit to the title if available
                    if let Some(limit) = self.token_limit {
                        title_format.token_limit = Some(limit);
                    }
                    ChatResponse::TaskMessage { content: ChatResponseContent::Title(title_format) }
                }
                x => x,
            };
            sender.send(Ok(agent_message)).await?
        }
        Ok(())
    }

    pub async fn send_text(&self, content: impl ToString) -> anyhow::Result<()> {
        self.send(ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText(content.to_string()),
        })
        .await
    }

    pub async fn send_title(&self, title: impl Into<TitleFormat>) -> anyhow::Result<()> {
        self.send(title.into()).await
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

    pub fn set_usage(&self, usage: Usage) -> anyhow::Result<()> {
        self.usage
            .lock()
            .ok()
            .context("Unable acquire lock for usage")?
            .replace(usage);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        let metrics = Metrics::new();
        let context = ToolCallContext::new(metrics);
        assert!(context.sender.is_none());
    }

    #[test]
    fn test_with_sender() {
        // This is just a type check test - we don't actually create a sender
        // as it's complex to set up in a unit test
        let metrics = Metrics::new();
        let context = ToolCallContext::new(metrics);
        assert!(context.sender.is_none());
    }
}
