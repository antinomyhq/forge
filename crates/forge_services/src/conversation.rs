use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_app::domain::{Agent, Conversation, ConversationId, Workflow};
use forge_app::{ConversationService, McpService};
use tokio::sync::Mutex;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService<M> {
    conversations: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
    mcp_service: Arc<M>,
}

impl<M: McpService> ForgeConversationService<M> {
    /// Creates a new ForgeConversationService with the provided MCP service
    pub fn new(mcp_service: Arc<M>) -> Self {
        Self {
            conversations: Arc::new(Mutex::new(HashMap::new())),
            mcp_service,
        }
    }
}

#[async_trait::async_trait]
impl<M: McpService> ConversationService for ForgeConversationService<M> {
    async fn modify_conversation<F, T>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        let mut conversation = self.conversations.lock().await;
        let conversation = conversation.get_mut(id).context("Conversation not found")?;
        Ok(f(conversation))
    }

    async fn find_conversation(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        Ok(self.conversations.lock().await.get(id).cloned())
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()> {
        self.conversations
            .lock()
            .await
            .insert(conversation.id, conversation);
        Ok(())
    }

    async fn init_conversation(
        &self,
        workflow: Workflow,
        agents: Vec<Agent>,
    ) -> Result<Conversation> {
        let id = ConversationId::generate();
        let conversation = Conversation::new(
            id,
            workflow,
            self.mcp_service
                .list()
                .await?
                .into_values()
                .flatten()
                .map(|tool| tool.name)
                .collect(),
            agents,
        );
        self.conversations
            .lock()
            .await
            .insert(id, conversation.clone());
        Ok(conversation)
    }
}
