use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use forge_app::domain::{Agent, Conversation, ConversationId, Workflow};
use forge_app::{ConversationService, McpService};

use crate::{
    EnvironmentInfra, FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra,
};


/// File name where last active conversation id is stored.
const LAST_ACTIVE_CONVERSATION_FILE_NAME: &str = ".last_active";

/// File-backed conversation service that persists conversations as JSON files
pub struct ForgeConversationService<M, I> {
    mcp_service: Arc<M>,
    infra: Arc<I>,
    conversation_dir: PathBuf,
}

impl<M: McpService, I> ForgeConversationService<M, I>
where
    I: EnvironmentInfra
        + FileReaderInfra
        + FileWriterInfra
        + FileDirectoryInfra
        + FileInfoInfra
        + Send
        + Sync
        + 'static,
{
    /// Creates a new FileConversationService instance
    pub fn new(mcp_service: Arc<M>, infra: Arc<I>) -> Self {
        let conversation_dir = infra.get_environment().conversation_path();
        let cwd = &infra.get_environment().cwd;

        // Generate workspace ID based on current working directory
        let mut hasher = DefaultHasher::new();
        cwd.hash(&mut hasher);
        let workspace_id = format!("{:x}", hasher.finish());

        Self {
            mcp_service,
            infra,
            conversation_dir: conversation_dir.join(workspace_id),
        }
    }

    fn conversation_path(&self, id: &ConversationId) -> PathBuf {
        self.conversation_dir
            .join(format!("{}.json", id.into_string()))
    }

    /// File path where the last active conversation id was stored.
    fn last_active_path(&self) -> PathBuf {
        self.conversation_dir.join(LAST_ACTIVE_CONVERSATION_FILE_NAME)
    }

    /// Saves a conversation to disk and updates the .latest file
    async fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        // Ensure conversation directory exists
        self.infra
            .create_dirs(&self.conversation_dir)
            .await
            .context("Failed to create workspace conversations directory")?;

        // Write conversation file
        let json = serde_json::to_string_pretty(conversation)
            .context("Failed to serialize conversation")?;

        self.infra
            .write(
                &self.conversation_path(&conversation.id),
                Bytes::from(json),
                false,
            )
            .await
            .context("Failed to write conversation file")?;

        // Update the .latest file
        self.infra
            .write(
                &self.last_active_path(),
                Bytes::from(conversation.id.to_string()),
                false,
            )
            .await
            .context("Failed to write workspace .latest file")
    }

    /// Loads a conversation from disk
    async fn load_conversation(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        let path = self.conversation_path(id);

        if !self.infra.exists(&path).await? {
            return Ok(None);
        }

        let content = self
            .infra
            .read_utf8(&path)
            .await
            .context("Failed to read conversation file")?;

        serde_json::from_str(&content)
            .map(Some)
            .context("Failed to parse conversation file")
    }
}

#[async_trait::async_trait]
impl<M: McpService, I> ConversationService for ForgeConversationService<M, I>
where
    I: EnvironmentInfra
        + FileReaderInfra
        + FileWriterInfra
        + FileDirectoryInfra
        + FileInfoInfra
        + Send
        + Sync
        + 'static,
{
    /// Modifies a conversation atomically and persists the changes
    async fn modify_conversation<F, T: Send>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        let mut conversation = self
            .load_conversation(id)
            .await?
            .with_context(|| format!("Conversation {id} not found"))?;

        let result = f(&mut conversation);
        self.save_conversation(&conversation).await?;
        Ok(result)
    }

    /// Finds a conversation by ID
    async fn find_conversation(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        self.load_conversation(id).await
    }

    /// Creates or updates a conversation
    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()> {
        self.save_conversation(&conversation).await
    }

    /// Initializes a new conversation with the given workflow and agents
    async fn init_conversation(
        &self,
        workflow: Workflow,
        agents: Vec<Agent>,
    ) -> Result<Conversation> {
        let id = ConversationId::generate();
        let tool_names = self
            .mcp_service
            .list()
            .await
            .context("Failed to retrieve tool list from MCP service")?
            .into_values()
            .flatten()
            .map(|tool| tool.name)
            .collect();

        let conversation = Conversation::new(id, workflow, tool_names, agents);
        self.save_conversation(&conversation).await?;
        Ok(conversation)
    }

    async fn find_last_active_conversation(&self) -> Result<Option<Conversation>> {
        if !self.infra.exists(&self.last_active_path()).await? {
            return Ok(None);
        }

        let conversation_id_str = self
            .infra
            .read_utf8(&self.last_active_path())
            .await
            .context(format!("Failed to read workspace {LAST_ACTIVE_CONVERSATION_FILE_NAME} file"))?;

        // Parse the conversation ID - if parsing fails, treat as if no latest
        // conversation exists
        let conversation_id = match ConversationId::parse(conversation_id_str.trim()) {
            Ok(id) => id,
            Err(_) => return Ok(None),
        };

        self.load_conversation(&conversation_id).await
    }
}
