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

/// Infrastructure requirements for FileConversationService
pub trait ConversationInfra:
    EnvironmentInfra
    + FileReaderInfra
    + FileWriterInfra
    + FileDirectoryInfra
    + FileInfoInfra
    + Send
    + Sync
    + 'static
{
}

impl<T> ConversationInfra for T where
    T: EnvironmentInfra
        + FileReaderInfra
        + FileWriterInfra
        + FileDirectoryInfra
        + FileInfoInfra
        + Send
        + Sync
        + 'static
{
}

/// File-backed conversation service that persists conversations as JSON files
pub struct FileConversationService<M, I> {
    mcp_service: Arc<M>,
    infra: Arc<I>,
    conversation_dir: PathBuf,
}

impl<M: McpService, I: ConversationInfra> FileConversationService<M, I> {
    /// Creates a new FileConversationService instance
    pub fn new(mcp_service: Arc<M>, infra: Arc<I>) -> Self {
        let conversation_dir = infra.get_environment().conversation_path();
        Self { mcp_service, infra, conversation_dir }
    }

    /// Generates a workspace ID based on the current working directory
    fn generate_workspace_id(&self) -> String {
        let cwd = &self.infra.get_environment().cwd;
        let mut hasher = DefaultHasher::new();
        cwd.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Returns the workspace directory for the current project
    fn workspace_dir(&self) -> PathBuf {
        let workspace_id = self.generate_workspace_id();
        self.conversation_dir.join(workspace_id)
    }

    /// Returns the file path for a specific conversation in the current
    /// workspace
    fn conversation_file_path(&self, id: &ConversationId) -> PathBuf {
        let workspace_dir = self.workspace_dir();
        workspace_dir.join(format!("{}.json", id.into_string()))
    }

    /// Saves a conversation to disk and updates the .latest file
    async fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        let workspace_dir = self.workspace_dir();
        self.infra
            .create_dirs(&workspace_dir)
            .await
            .context("Failed to create workspace conversations directory")?;
        let path = self.conversation_file_path(&conversation.id);
        let json = serde_json::to_string_pretty(conversation)
            .context("Failed to serialize conversation")?;
        self.infra
            .write(&path, Bytes::from(json), false)
            .await
            .context("Failed to write conversation file")?;

        // Update the .latest file to track this as the most recent conversation
        self.update_latest_conversation(&conversation.id).await
    }

    /// Loads a conversation from disk
    async fn load_conversation(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        let path = self.conversation_file_path(id);
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

    /// Gets a conversation, returning an error if it doesn't exist
    async fn require_conversation(&self, id: &ConversationId) -> Result<Conversation> {
        self.load_conversation(id)
            .await?
            .with_context(|| format!("Conversation {id} not found"))
    }

    /// Gets the last active conversation from the workspace-specific .latest
    /// file
    async fn find_latest_conversation(&self) -> Result<Option<Conversation>> {
        let workspace_dir = self.workspace_dir();
        let latest_file_path = workspace_dir.join(".latest");

        // Check if the workspace .latest file exists
        if !self.infra.exists(&latest_file_path).await? {
            return Ok(None);
        }

        // Read the conversation ID from the workspace .latest file
        let conversation_id_str = self
            .infra
            .read_utf8(&latest_file_path)
            .await
            .context("Failed to read workspace .latest file")?
            .trim()
            .to_string();

        // Parse the conversation ID and load the conversation
        match ConversationId::parse(&conversation_id_str) {
            Ok(conversation_id) => self.load_conversation(&conversation_id).await,
            Err(_) => Ok(None),
        }
    }

    /// Updates the workspace-specific .latest file with the given conversation
    /// ID
    async fn update_latest_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        let workspace_dir = self.workspace_dir();
        self.infra
            .create_dirs(&workspace_dir)
            .await
            .context("Failed to create workspace conversations directory")?;

        let latest_file_path = workspace_dir.join(".latest");
        let content = conversation_id.to_string();

        self.infra
            .write(&latest_file_path, Bytes::from(content), false)
            .await
            .context("Failed to write workspace .latest file")
    }
}

#[async_trait::async_trait]
impl<M: McpService, I: ConversationInfra> ConversationService for FileConversationService<M, I> {
    /// Modifies a conversation atomically and persists the changes
    async fn modify_conversation<F, T: Send>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        // Load conversation
        let mut conversation = self.require_conversation(id).await?;

        // Apply modification
        let result = f(&mut conversation);

        // Save the modified conversation (this will also update .latest)
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
        self.find_latest_conversation().await
    }
}
