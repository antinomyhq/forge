use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_domain::{Conversation, ConversationId, ConversationService, Workflow};
use serde_json::Value;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ForgeConversationService {
    workflows: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
}

impl Default for ForgeConversationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeConversationService {
    pub fn new() -> Self {
        Self { workflows: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl ConversationService for ForgeConversationService {
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
            .insert(conversation.id.clone(), conversation);
        Ok(())
    }

    async fn create(&self, workflow: Workflow) -> Result<ConversationId> {
        let id = ConversationId::generate();
        let conversation = Conversation::new(id.clone(), workflow);
        self.workflows.lock().await.insert(id.clone(), conversation);
        Ok(id)
    }

    async fn get_variable(&self, id: &ConversationId, key: &str) -> Result<Option<Value>> {
        self.update(id, |c| c.get_variable(key).cloned()).await
    }

    async fn set_variable(&self, id: &ConversationId, key: String, value: Value) -> Result<()> {
        self.update(id, |c| {
            c.set_variable(key, value);
        })
        .await
    }

    async fn delete_variable(&self, id: &ConversationId, key: &str) -> Result<bool> {
        self.update(id, |c| c.delete_variable(key)).await
    }
}
