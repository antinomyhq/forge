use std::collections::HashMap;
use std::sync::Arc;

use forge_domain::{
    AgentId, Context, Conversation, ConversationId, ConversationService, Event, Workflow,
};
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
}

#[async_trait::async_trait]
impl ConversationService for ForgeConversationService {
    async fn get(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>> {
        Ok(self.workflows.lock().await.get(id).cloned())
    }

    async fn create(&self, workflow: Workflow) -> anyhow::Result<ConversationId> {
        let id = ConversationId::generate();
        let conversation = Conversation::new(id.clone(), workflow);
        self.workflows.lock().await.insert(id.clone(), conversation);
        Ok(id)
    }

    async fn inc_turn(&self, id: &ConversationId, agent: &AgentId) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<()> {
        if let Some(c) = self.workflows.lock().await.get_mut(id) {
            c.state.entry(agent.clone()).or_default().context = Some(context);
        }
        Ok(())
    }

    async fn insert_event(&self, id: &ConversationId, event: Event) -> anyhow::Result<()> {
        let mut guard = self.workflows.lock().await;
        guard
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Conversation not found"))?
            .events
            .push(event);
        Ok(())
    }
}
