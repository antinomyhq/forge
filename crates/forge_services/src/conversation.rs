#![allow(dead_code)]
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
    async fn find_last_active_conversation(&self) -> Result<Option<Conversation>> {
        let conversations = self.conversations.lock().await;

        if conversations.is_empty() {
            return Ok(None);
        }

        // Find the conversation with the most recent event timestamp
        let mut latest_conversation: Option<&Conversation> = None;
        let mut latest_timestamp: Option<String> = None;

        for conversation in conversations.values() {
            if let Some(latest_event) = conversation.events.last() {
                match &latest_timestamp {
                    None => {
                        latest_timestamp = Some(latest_event.timestamp.clone());
                        latest_conversation = Some(conversation);
                    }
                    Some(current_latest) => {
                        if latest_event.timestamp > *current_latest {
                            latest_timestamp = Some(latest_event.timestamp.clone());
                            latest_conversation = Some(conversation);
                        }
                    }
                }
            } else if latest_conversation.is_none() {
                // If no events, use the first conversation found as fallback
                latest_conversation = Some(conversation);
            }
        }

        Ok(latest_conversation.cloned())
    }
}
