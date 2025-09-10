use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_app::ConversationService;
use forge_app::domain::{Conversation, ConversationId};
use tokio::sync::Mutex;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService {
    conversations: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
}

impl Default for ForgeConversationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeConversationService {
    /// Creates a new ForgeConversationService with the provided MCP service
    pub fn new() -> Self {
        Self { conversations: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl ConversationService for ForgeConversationService {
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

    async fn init_conversation(&self) -> Result<Conversation> {
        let id = ConversationId::generate();

        let conversation = Conversation::new(id);

        self.conversations
            .lock()
            .await
            .insert(id, conversation.clone());
        Ok(conversation)
    }
}
