use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use derive_setters::Setters;
use serde::Serialize;

use crate::{ArcSender, ChatResponse, ChatResponseContent, Metrics, TitleFormat};

/// Provides additional context for tool calls.
#[derive(Debug, Clone, Setters)]
pub struct ToolCallContext {
    sender: Option<ArcSender>,
    metrics: Arc<Mutex<Metrics>>,
    active_plan: Arc<Mutex<Option<ActivePlan>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivePlan {
    pub path: PathBuf,
    pub stat: PlanStat,
}

impl ActivePlan {
    /// Check if the plan is complete (all tasks are done and no tasks are
    /// pending or in progress)
    pub fn is_complete(&self) -> bool {
        self.stat.todo == 0 && self.stat.in_progress == 0 && self.stat.failed == 0
    }

    pub fn complete_percentage(&self) -> f32 {
        self.stat.completed as f32 / self.stat.total() as f32
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStat {
    pub completed: usize,
    pub todo: usize,
    pub failed: usize,
    pub in_progress: usize,
}

impl PlanStat {
    /// Calculate the total number of tasks
    pub fn total(&self) -> usize {
        self.completed + self.todo + self.failed + self.in_progress
    }
}

impl ToolCallContext {
    /// Creates a new ToolCallContext with default values
    pub fn new(metrics: Metrics) -> Self {
        Self {
            sender: None,
            metrics: Arc::new(Mutex::new(metrics)),
            active_plan: Arc::new(Mutex::new(None)),
        }
    }

    /// Send a message through the sender if available
    pub async fn send(&self, agent_message: impl Into<ChatResponse>) -> anyhow::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(Ok(agent_message.into())).await?
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

    /// Set the active plan path
    pub fn set_active_plan(&self, plan: ActivePlan) -> anyhow::Result<()> {
        let mut active_plan = self
            .active_plan
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire active_plan lock"))?;
        *active_plan = Some(plan);
        Ok(())
    }

    /// Get the active plan path
    pub fn get_active_plan(&self) -> anyhow::Result<Option<ActivePlan>> {
        let active_plan = self
            .active_plan
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire active_plan lock"))?;
        Ok(active_plan.clone())
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
