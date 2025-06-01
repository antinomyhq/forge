use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_domain::{
    estimate_token_count, AgentId, CompactionResult, CompactionService, Conversation,
    ConversationId, ConversationService, ConversationSessionManager, McpService, Workflow,
};
use tokio::sync::Mutex;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService<C, M, Manager> {
    workflows: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
    compaction_service: Arc<C>,
    mcp_service: Arc<M>,
    conversation_session_manager: Arc<Manager>,
}

impl<C: CompactionService, M: McpService, Manager: ConversationSessionManager>
    ForgeConversationService<C, M, Manager>
{
    /// Creates a new ForgeConversationService with the provided compaction
    /// service
    pub fn new(
        compaction_service: Arc<C>,
        mcp_service: Arc<M>,
        conversation_session_manager: Arc<Manager>,
    ) -> Self {
        Self {
            workflows: Arc::new(Mutex::new(HashMap::new())),
            compaction_service,
            mcp_service,
            conversation_session_manager,
        }
    }
}

#[async_trait::async_trait]
impl<C: CompactionService, M: McpService, Manager: ConversationSessionManager> ConversationService
    for ForgeConversationService<C, M, Manager>
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

    async fn compact_conversation(&self, id: &ConversationId) -> Result<CompactionResult> {
        // Fetch the conversation
        let mut conversation = self
            .find(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Conversation not found"))?;

        // Identify the main agent and extract existing context
        let main_agent_id = AgentId::new(Conversation::MAIN_AGENT_NAME);
        let agent = conversation.get_agent(&main_agent_id)?;
        let context = conversation
            .state
            .get(&main_agent_id)
            .and_then(|s| s.context.clone())
            .unwrap_or_default();

        // Compute original metrics
        let original_tokens = estimate_token_count(context.to_text().len());
        let original_messages = context.messages.len();

        // Perform compaction
        let new_context = self
            .compaction_service
            .compact_context(agent, context.clone())
            .await?;

        // Compute compacted metrics
        let compacted_tokens = estimate_token_count(new_context.to_text().len());
        let compacted_messages = new_context.messages.len();

        // Persist the updated context
        conversation
            .state
            .entry(main_agent_id.clone())
            .or_default()
            .context = Some(new_context.clone());
        self.upsert(conversation).await?;

        // Return metrics
        Ok(CompactionResult::new(
            original_tokens,
            compacted_tokens,
            original_messages,
            compacted_messages,
        ))
    }
}
