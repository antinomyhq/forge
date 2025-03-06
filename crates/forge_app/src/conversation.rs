use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use forge_domain::{
    AgentId, Context, Conversation, ConversationId, ConversationService, Event, Workflow,
};
use serde_json::Value;
use tokio::sync::Mutex;

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

    // Helper method for operations requiring mutable access to a conversation
    async fn write<F, T>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T,
    {
        let mut guard = self.workflows.lock().await;
        let conversation = guard
            .get_mut(id)
            .ok_or_else(|| anyhow!("Conversation not found"))?;
        Ok(f(conversation))
    }

    // Helper method for operations requiring immutable access to a conversation
    async fn read<F, T>(&self, id: &ConversationId, f: F) -> Result<Option<T>>
    where
        F: FnOnce(&Conversation) -> Option<T>,
    {
        let guard = self.workflows.lock().await;
        Ok(guard.get(id).and_then(f))
    }
}

#[async_trait::async_trait]
impl ConversationService for ForgeConversationService {
    async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        Ok(self.workflows.lock().await.get(id).cloned())
    }

    async fn create(&self, workflow: Workflow) -> Result<ConversationId> {
        let id = ConversationId::generate();
        let conversation = Conversation::new(id.clone(), workflow);
        self.workflows.lock().await.insert(id.clone(), conversation);
        Ok(id)
    }

    async fn inc_turn(&self, id: &ConversationId, agent: &AgentId) -> Result<()> {
        if let Some(c) = self.workflows.lock().await.get_mut(id) {
            c.state.entry(agent.clone()).or_default().turn_count += 1;
        }
        Ok(())
    }

    async fn set_context(
        &self,
        id: &ConversationId,
        agent: &AgentId,
        context: Context,
    ) -> Result<()> {
        if let Some(c) = self.workflows.lock().await.get_mut(id) {
            c.state.entry(agent.clone()).or_default().context = Some(context);
        }
        Ok(())
    }

    async fn insert_event(&self, id: &ConversationId, event: Event) -> Result<()> {
        self.write(id, |c| {
            c.events.push(event);
        })
        .await
    }

    async fn get_variable(&self, id: &ConversationId, key: &str) -> Result<Option<Value>> {
        self.read(id, |c| c.get_variable(key).cloned()).await
    }

    async fn set_variable(&self, id: &ConversationId, key: String, value: Value) -> Result<()> {
        self.write(id, |c| {
            c.set_variable(key, value);
        })
        .await
    }

    async fn delete_variable(&self, id: &ConversationId, key: &str) -> Result<bool> {
        self.write(id, |c| c.delete_variable(key)).await
    }
}
