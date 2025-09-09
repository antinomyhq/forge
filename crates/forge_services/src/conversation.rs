use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::domain::{Agent, Conversation, ConversationId, Workflow};
use forge_app::{ConversationService, McpService};

use crate::{ConversationStorageInfra, EnvironmentInfra};

/// Database-backed conversation service that persists conversations via storage
/// infrastructure
pub struct ForgeConversationService<M, I> {
    mcp_service: Arc<M>,
    infra: Arc<I>,
}

impl<M: McpService, I> ForgeConversationService<M, I>
where
    I: EnvironmentInfra + ConversationStorageInfra + Send + Sync + 'static,
{
    /// Creates a new ForgeConversationService instance
    pub fn new(mcp_service: Arc<M>, infra: Arc<I>) -> Self {
        Self { mcp_service, infra }
    }

    fn get_workspace_id(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.infra.get_environment().cwd.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[async_trait]
impl<M: McpService, I> ConversationService for ForgeConversationService<M, I>
where
    I: EnvironmentInfra + ConversationStorageInfra + Send + Sync + 'static,
{
    async fn find_conversation(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        self.infra
            .find_by_id(&id.into_string())
            .await
            .context("Failed to find conversation")
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()> {
        self.infra
            .upsert(&conversation)
            .await
            .context("Failed to upsert conversation")
    }

    async fn init_conversation(
        &self,
        workflow: Workflow,
        agents: Vec<Agent>,
    ) -> Result<Conversation> {
        let id = ConversationId::generate();
        let workspace_id = self.get_workspace_id();
        let tool_names = self
            .mcp_service
            .list()
            .await
            .context("Failed to retrieve tool list from MCP service")?
            .into_values()
            .flatten()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        let conversation = Conversation::new(id, workspace_id, workflow, tool_names, agents);
        self.upsert_conversation(conversation.clone()).await?;
        Ok(conversation)
    }

    async fn modify_conversation<F, T: Send>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        let mut conversation = self
            .find_conversation(id)
            .await?
            .with_context(|| format!("Conversation {} not found", id.into_string()))?;

        let result = f(&mut conversation);
        self.upsert_conversation(conversation).await?;
        Ok(result)
    }

    async fn find_last_active_conversation(&self) -> Result<Option<Conversation>> {
        let workspace_id = self.get_workspace_id();

        self.infra
            .find_latest_by_workspace_id(&workspace_id)
            .await
            .context("Failed to find latest conversation")
    }

    async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let workspace_id = self.get_workspace_id();
        self.infra
            .find_by_workspace_id(&workspace_id)
            .await
            .context("Failed to list conversations")
    }
}
