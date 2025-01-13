use async_trait::async_trait;

use crate::{Conversation, ConversationId};

#[async_trait]
pub trait ConversationRepository {
    /// Get a conversation by its ID
    async fn get_conversation(&self, id: ConversationId) -> anyhow::Result<Option<Conversation>>;
    
    /// Save a new conversation or update an existing one
    async fn save_conversation(&self, conversation: &Conversation) -> anyhow::Result<()>;
    
    /// List all conversations
    async fn list_conversations(&self) -> anyhow::Result<Vec<Conversation>>;
    
    /// Archive a conversation
    async fn archive_conversation(&self, id: ConversationId) -> anyhow::Result<()>;
}