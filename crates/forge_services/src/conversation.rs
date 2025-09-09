use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use forge_app::domain::{Agent, Conversation, ConversationId, Workflow};
use forge_app::{ConversationService, McpService};

use crate::{
    DirectoryReaderInfra, EnvironmentInfra, FileDirectoryInfra, FileInfoInfra, FileReaderInfra,
    FileWriterInfra,
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
        + DirectoryReaderInfra
        + Send
        + Sync
        + 'static,
{
    /// Creates a new FileConversationService instance
    pub fn new(mcp_service: Arc<M>, infra: Arc<I>) -> Self {
        let conversation_dir = infra.get_environment().conversation_path();
        Self { mcp_service, infra, conversation_dir }
    }

    fn conversation_path(&self, id: &ConversationId) -> PathBuf {
        self.conversation_dir
            .join(format!("{}.json", id.into_string()))
    }

    /// File path where the last active conversation id was stored.
    fn last_active_path(&self) -> PathBuf {
        self.conversation_dir
            .join(LAST_ACTIVE_CONVERSATION_FILE_NAME)
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

        // Update the LAST_ACTIVE_CONVERSATION_FILE_NAME file
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
        + DirectoryReaderInfra
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
            .context(format!(
                "Failed to read workspace {LAST_ACTIVE_CONVERSATION_FILE_NAME} file"
            ))?;

        // Parse the conversation ID - if parsing fails, treat as if no latest
        // conversation exists
        let conversation_id = match ConversationId::parse(conversation_id_str.trim()) {
            Ok(id) => id,
            Err(_) => return Ok(None),
        };

        self.load_conversation(&conversation_id).await
    }

    /// Loads all conversation ids from the storage layer.
    async fn list_conversations(&self) -> anyhow::Result<Vec<Conversation>> {
        // Check if conversation directory exists
        if !self.infra.exists(&self.conversation_dir).await? {
            return Ok(Vec::new());
        }

        // Read all files in the conversation directory
        let files = self
            .infra
            .read_directory_files(&self.conversation_dir, Some("*.json"))
            .await
            .context("Failed to read conversations directory")?;

        let mut conversations = Vec::with_capacity(files.len());
        for (_, _content) in files {
            if let Ok(conversation) = serde_json::from_str::<Conversation>(&_content) {
                conversations.push(conversation);
            }
        }

        Ok(conversations)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use anyhow::{Result, anyhow};
    use forge_app::domain::{
        Agent, Conversation, ConversationId, ToolCallFull, ToolDefinition, ToolName, ToolOutput,
        Workflow,
    };
    use forge_app::{ConversationService, McpService};
    use pretty_assertions::assert_eq;

    use crate::attachment::tests::MockCompositeService;
    use crate::{FileInfoInfra, FileReaderInfra, ForgeConversationService};

    // Mock MCP service for testing
    struct MockMcpService {
        tools: HashMap<String, Vec<ToolDefinition>>,
        should_fail_list: bool,
    }

    impl Default for MockMcpService {
        fn default() -> Self {
            let tools = HashMap::from([
                (
                    "server1".to_string(),
                    vec![
                        ToolDefinition {
                            name: ToolName::new("tool1"),
                            description: "Test tool 1".to_string(),
                            input_schema: schemars::schema_for!(String),
                        },
                        ToolDefinition {
                            name: ToolName::new("tool2"),
                            description: "Test tool 2".to_string(),
                            input_schema: schemars::schema_for!(String),
                        },
                    ],
                ),
                (
                    "server2".to_string(),
                    vec![ToolDefinition {
                        name: ToolName::new("tool3"),
                        description: "Test tool 3".to_string(),
                        input_schema: schemars::schema_for!(String),
                    }],
                ),
            ]);

            Self { tools, should_fail_list: false }
        }
    }

    #[async_trait::async_trait]
    impl McpService for MockMcpService {
        async fn list(&self) -> Result<HashMap<String, Vec<ToolDefinition>>> {
            if self.should_fail_list {
                return Err(anyhow!("Mock MCP list failure"));
            }
            Ok(self.tools.clone())
        }

        async fn call(&self, _call: ToolCallFull) -> Result<ToolOutput> {
            Ok(ToolOutput::text("mock output".to_string()))
        }
    }

    // Test fixtures
    fn conversation_fixture() -> Conversation {
        let id = ConversationId::generate();
        let workflow = Workflow::new();
        let agents = vec![Agent::new("test-agent")];
        Conversation::new(id, workflow, vec![ToolName::new("test-tool")], agents)
    }

    fn service_fixture() -> ForgeConversationService<MockMcpService, MockCompositeService> {
        ForgeConversationService::new(
            Arc::new(MockMcpService::default()),
            Arc::new(MockCompositeService::new()),
        )
    }

    #[tokio::test]
    async fn test_upsert_conversation_saves_and_updates_last_active() {
        let fixture = conversation_fixture();
        let service = service_fixture();

        let actual = service.upsert_conversation(fixture.clone()).await;

        assert!(actual.is_ok());
        let conversation_path = service.conversation_path(&fixture.id);
        assert!(service.infra.exists(&conversation_path).await.unwrap());

        let last_active_path = service.last_active_path();
        assert!(service.infra.exists(&last_active_path).await.unwrap());
        let saved_id = service.infra.read_utf8(&last_active_path).await.unwrap();
        assert_eq!(saved_id, fixture.id.to_string());
    }

    #[tokio::test]
    async fn test_find_conversation_returns_existing_conversation() {
        let fixture = conversation_fixture();
        let id = fixture.id.clone();
        let conversation_json = serde_json::to_string_pretty(&fixture).unwrap();

        let service = service_fixture();
        service
            .infra
            .add_file(service.conversation_path(&id), conversation_json);

        let actual = service.find_conversation(&id).await.unwrap();

        assert!(actual.is_some());
        let found_conversation = actual.unwrap();
        assert_eq!(found_conversation.id, fixture.id);
        assert_eq!(found_conversation.agents.len(), fixture.agents.len());
    }

    #[tokio::test]
    async fn test_init_conversation() {
        let workflow = Workflow::new();
        let agents = vec![Agent::new("test-agent")];
        let service = service_fixture();

        let actual = service.init_conversation(workflow, agents).await.unwrap();
        let last_active_path = service.last_active_path();
        assert!(service.infra.exists(&last_active_path).await.unwrap());
        let saved_id = service.infra.read_utf8(&last_active_path).await.unwrap();
        assert_eq!(actual.id.into_string(), saved_id);
    }

    #[tokio::test]
    async fn test_modify_conversation_applies_changes_and_persists() {
        let mut fixture = conversation_fixture();
        fixture.archived = false;
        let id = fixture.id.clone();
        let conversation_json = serde_json::to_string_pretty(&fixture).unwrap();

        let service = service_fixture();
        service
            .infra
            .add_file(service.conversation_path(&id), conversation_json);

        let actual = service
            .modify_conversation(&id, |conversation| {
                conversation.archived = true;
                "modified"
            })
            .await
            .unwrap();

        assert_eq!(actual, "modified");

        // Verify the conversation was persisted with changes
        let saved_conversation = service.find_conversation(&id).await.unwrap().unwrap();
        assert_eq!(saved_conversation.archived, true);
    }

    #[tokio::test]
    async fn test_modify_conversation_fails_when_conversation_not_found() {
        let service = service_fixture();
        let id = ConversationId::generate();

        let actual = service.modify_conversation(&id, |_| "test").await;

        assert!(actual.is_err());
        let error_msg = actual.unwrap_err().to_string();
        assert!(error_msg.contains(&format!("Conversation {} not found", id)));
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_returns_existing_conversation() {
        let fixture = conversation_fixture();
        let id = fixture.id.clone();
        let conversation_json = serde_json::to_string_pretty(&fixture).unwrap();

        let service = service_fixture();
        service
            .infra
            .add_file(service.last_active_path(), id.to_string());
        service
            .infra
            .add_file(service.conversation_path(&id), conversation_json);

        let actual = service.find_last_active_conversation().await.unwrap();

        assert!(actual.is_some());
        let found_conversation = actual.unwrap();
        assert_eq!(found_conversation.id, fixture.id);
    }
    #[tokio::test]
    async fn test_list_conversations_returns_all_conversation_ids() {
        let fixture1 = conversation_fixture();
        let fixture2 = conversation_fixture();
        let service = service_fixture();

        // Add conversation files
        let conversation_json1 = serde_json::to_string_pretty(&fixture1).unwrap();
        let conversation_json2 = serde_json::to_string_pretty(&fixture2).unwrap();

        service
            .infra
            .add_file(service.conversation_path(&fixture1.id), conversation_json1);
        service
            .infra
            .add_file(service.conversation_path(&fixture2.id), conversation_json2);

        // Add the .last_active file (should be ignored)
        service
            .infra
            .add_file(service.last_active_path(), fixture1.id.to_string());

        let actual = service.list_conversations().await.unwrap();

        assert_eq!(actual.len(), 2);
        assert!(actual.iter().any(|conv| conv.id == fixture1.id));
        assert!(actual.iter().any(|conv| conv.id == fixture2.id));
    }

    #[tokio::test]
    async fn test_list_conversations_returns_empty_when_directory_does_not_exist() {
        let service = service_fixture();
        let actual = service.list_conversations().await.unwrap();
        assert!(actual.is_empty());
    }

    #[tokio::test]
    async fn test_list_conversations_skips_invalid_files() {
        let fixture = conversation_fixture();
        let service = service_fixture();

        // Add valid conversation file
        let conversation_json = serde_json::to_string_pretty(&fixture).unwrap();
        service
            .infra
            .add_file(service.conversation_path(&fixture.id), conversation_json);

        // Add invalid files that should be ignored
        service.infra.add_file(
            service.conversation_dir.join("invalid.txt"),
            "not json".to_string(),
        );
        service.infra.add_file(
            service.conversation_dir.join("invalid-id.json"),
            "{}".to_string(),
        );
        service
            .infra
            .add_file(service.last_active_path(), fixture.id.to_string());

        let actual = service.list_conversations().await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, fixture.id);
    }
}
