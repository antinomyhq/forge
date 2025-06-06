use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_domain::{CompactionResult, Conversation, ConversationId, Workflow};
use tokio::sync::Mutex;

use crate::services::{ConversationService, McpService};
use crate::ConversationSessionManager;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService<M, Manager> {
    workflows: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
    mcp_service: Arc<M>,
    conversation_session_manager: Arc<Manager>,
}

impl<M: McpService, Manager: ConversationSessionManager> ForgeConversationService<M, Manager> {
    /// Creates a new ForgeConversationService with the provided MCP service
    pub fn new(mcp_service: Arc<M>, conversation_session_manager: Arc<Manager>) -> Self {
        Self {
            workflows: Arc::new(Mutex::new(HashMap::new())),
            mcp_service,
            conversation_session_manager,
        }
    }
}

#[async_trait::async_trait]
impl<M: McpService, Manager: ConversationSessionManager> ConversationService
    for ForgeConversationService<M, Manager>
{
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
        self.conversation_session_manager
            .conversation_update(&conversation)
            .await?;

        self.workflows
            .lock()
            .await
            .insert(conversation.id.clone(), conversation);
        Ok(())
    }

    async fn create(&self, workflow: Workflow) -> Result<Conversation> {
        let id = ConversationId::generate();
        let conversation = Conversation::new(
            id.clone(),
            workflow,
            self.mcp_service
                .list()
                .await?
                .into_iter()
                .map(|a| a.name)
                .collect(),
        );
        self.upsert(conversation.clone()).await?;
        Ok(conversation)
    }

    async fn compact_conversation(&self, _id: &ConversationId) -> Result<CompactionResult> {
        // Since compaction is now handled directly in the Orchestrator,
        // this method now just returns a dummy result indicating no compaction was
        // performed In a real implementation, this functionality could be moved
        // to the Orchestrator or removed entirely if not needed at the service
        // level
        Ok(CompactionResult::new(0, 0, 0, 0))
    }
}
