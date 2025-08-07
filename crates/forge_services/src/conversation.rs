use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_app::domain::{Conversation, ConversationId, Workflow};
use forge_app::{ConversationService, McpService};
use merge::Merge;
use tokio::sync::Mutex;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService<M> {
    workflows: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
    mcp_service: Arc<M>,
}

impl<M: McpService> ForgeConversationService<M> {
    /// Creates a new ForgeConversationService with the provided MCP service
    pub fn new(mcp_service: Arc<M>) -> Self {
        Self { workflows: Arc::new(Mutex::new(HashMap::new())), mcp_service }
    }
}

#[async_trait::async_trait]
impl<M: McpService> ConversationService for ForgeConversationService<M> {
    async fn update<F, T>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        let mut workflows = self.workflows.lock().await;
        let conversation = workflows.get_mut(id).context("Conversation not found")?;
        Ok(f(conversation))
    }

    async fn find(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        Ok(self.workflows.lock().await.get(id).cloned())
    }

    async fn upsert(&self, conversation: Conversation) -> Result<()> {
        self.workflows
            .lock()
            .await
            .insert(conversation.id, conversation);
        Ok(())
    }

    async fn create_conversation(&self, given_workflow: Workflow) -> Result<Conversation> {
        let mut workflow = Workflow::default();
        workflow.merge(given_workflow);
        let id = ConversationId::generate();
        let conversation = Conversation::new(
            id,
            workflow,
            self.mcp_service
                .list()
                .await?
                .into_iter()
                .map(|a| a.name)
                .collect(),
        );
        self.workflows.lock().await.insert(id, conversation.clone());
        Ok(conversation)
    }
}
