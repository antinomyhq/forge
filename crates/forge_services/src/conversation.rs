use std::sync::Arc;

use anyhow::Result;
use forge_app::ConversationService;
use forge_app::domain::{Conversation, ConversationId, WorkspaceId};

use crate::ConversationRepository;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService<S> {
    conversation_repository: Arc<S>,
    workspace_id: WorkspaceId,
}

impl<S: ConversationRepository> ForgeConversationService<S> {
    /// Creates a new ForgeConversationService with the provided MCP service
    pub fn new(workspace_id: WorkspaceId, repo: Arc<S>) -> Self {
        Self { conversation_repository: repo, workspace_id }
    }
}

#[async_trait::async_trait]
impl<S: ConversationRepository> ConversationService for ForgeConversationService<S> {
    async fn modify_conversation<F, T>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
        T: Send,
    {
        let mut conversation = self
            .conversation_repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found: {}", id))?;
        let out = f(&mut conversation);
        let _ = self.conversation_repository.upsert(conversation).await?;
        Ok(out)
    }

    async fn find_conversation(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        self.conversation_repository.find_by_id(id).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()> {
        let _ = self.conversation_repository.upsert(conversation).await?;
        Ok(())
    }

    async fn init_conversation(&self) -> Result<Conversation> {
        let id = ConversationId::generate();
        let conversation = Conversation::new(id, self.workspace_id.clone());
        let _ = self
            .conversation_repository
            .upsert(conversation.clone())
            .await?;
        Ok(conversation)
    }

    async fn find_conversations(
        &self,
        workspace_id: &WorkspaceId,
        limit: Option<usize>,
    ) -> Result<Option<Vec<Conversation>>> {
        self.conversation_repository
            .find_by_workspace_id(workspace_id, limit)
            .await
    }

    async fn last_conversation(&self, workspace_id: &WorkspaceId) -> Result<Option<Conversation>> {
        self.conversation_repository
            .find_last_active_conversation_by_workspace_id(workspace_id)
            .await
    }
}
