use std::path::{Path, PathBuf};

use anyhow::Result;
use forge_app::dto::ToolsOverview;
use forge_app::{User, UserUsage};
use forge_domain::{AgentId, InitAuth, ModelId};
use forge_stream::MpscStream;
use futures::stream::BoxStream;
use url::Url;

use crate::*;

#[async_trait::async_trait]
pub trait API: Sync + Send {
    /// Provides a list of files in the current working directory for auto
    /// completion
    async fn discover(&self) -> Result<Vec<crate::File>>;

    /// Provides information about the tools available in the current
    /// environment
    async fn get_tools(&self) -> anyhow::Result<ToolsOverview>;

    /// Provides a list of models available in the current environment
    async fn get_models(&self) -> Result<Vec<Model>>;
    /// Provides a list of agents available in the current environment
    async fn get_agents(&self) -> Result<Vec<Agent>>;
    /// Provides a list of providers available in the current environment
    async fn get_providers(&self) -> Result<Vec<AnyProvider>>;

    /// Gets a provider by ID
    async fn get_provider(&self, id: &ProviderId) -> Result<AnyProvider>;

    /// Executes a chat request and returns a stream of responses
    async fn chat(&self, chat: ChatRequest) -> Result<MpscStream<Result<ChatResponse>>>;

    /// Commits changes with an AI-generated commit message
    async fn commit(
        &self,
        preview: bool,
        max_diff_size: Option<usize>,
        diff: Option<String>,
        additional_context: Option<String>,
    ) -> Result<forge_app::CommitResult>;

    /// Returns the current environment
    fn environment(&self) -> Environment;

    /// Adds a new conversation to the conversation store
    async fn upsert_conversation(&self, conversation: Conversation) -> Result<()>;

    /// Initializes a workflow configuration from the given path
    /// The workflow at the specified path is merged with the default
    /// configuration If no path is provided, it will try to find forge.yaml
    /// in the current directory or its parent directories
    async fn read_workflow(&self, path: Option<&Path>) -> Result<Workflow>;

    /// Reads the workflow from the given path and merges it with a default
    /// workflow. This provides a convenient way to get a complete workflow
    /// configuration without having to manually handle the merge logic.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories
    async fn read_merged(&self, path: Option<&Path>) -> Result<Workflow>;

    /// Returns the conversation with the given ID
    async fn conversation(&self, conversation_id: &ConversationId) -> Result<Option<Conversation>>;

    /// Lists all conversations for the active workspace
    async fn get_conversations(&self, limit: Option<usize>) -> Result<Vec<Conversation>>;

    /// Finds the last active conversation for the current workspace
    async fn last_conversation(&self) -> Result<Option<Conversation>>;

    /// Permanently deletes a conversation
    ///
    /// # Arguments
    /// * `conversation_id` - The ID of the conversation to delete
    ///
    /// # Errors
    /// Returns an error if the operation fails
    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()>;

    /// Compacts the context of the main agent for the given conversation and
    /// persists it. Returns metrics about the compaction (original vs.
    /// compacted tokens and messages).
    async fn compact_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<CompactionResult>;

    /// Executes a shell command using the shell tool infrastructure
    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: PathBuf,
    ) -> Result<CommandOutput>;

    /// Executes the shell command on present stdio.
    async fn execute_shell_command_raw(&self, command: &str) -> Result<std::process::ExitStatus>;

    /// Reads and merges MCP configurations from all available configuration
    /// files This combines both user-level and local configurations with
    /// local taking precedence. If scope is provided, only loads from that
    /// specific scope.
    async fn read_mcp_config(&self, scope: Option<&Scope>) -> Result<McpConfig>;

    /// Writes the provided MCP configuration to disk at the specified scope
    /// The scope determines whether the configuration is written to user-level
    /// or local configuration User-level configuration is stored in the
    /// user's home directory Local configuration is stored in the current
    /// project directory
    async fn write_mcp_config(&self, scope: &Scope, config: &McpConfig) -> Result<()>;

    /// Initiates the login flow and returns authentication initialization data
    async fn init_login(&self) -> Result<InitAuth>;

    /// Retrieves the current login information if the user is authenticated
    async fn get_login_info(&self) -> Result<Option<LoginInfo>>;

    /// Completes the login process using the provided authentication data
    async fn login(&self, auth: &InitAuth) -> Result<()>;

    /// Logs out the current user and clears authentication data
    async fn logout(&self) -> anyhow::Result<()>;

    /// Retrieves the provider configuration for the specified agent
    async fn get_agent_provider(&self, agent_id: AgentId) -> anyhow::Result<Provider<Url>>;

    /// Retrieves the provider configuration for the default agent
    async fn get_default_provider(&self) -> anyhow::Result<Provider<Url>>;

    /// Sets the default provider for all the agents
    async fn set_default_provider(&self, provider_id: ProviderId) -> anyhow::Result<()>;

    /// Retrieves information about the currently authenticated user
    async fn user_info(&self) -> anyhow::Result<Option<User>>;

    /// Retrieves usage statistics for the currently authenticated user
    async fn user_usage(&self) -> anyhow::Result<Option<UserUsage>>;

    /// Gets the currently operating agent
    async fn get_active_agent(&self) -> Option<AgentId>;

    /// Sets the active agent
    async fn set_active_agent(&self, agent_id: AgentId) -> anyhow::Result<()>;

    /// Gets the model for the specified agent
    async fn get_agent_model(&self, agent_id: AgentId) -> Option<ModelId>;

    /// Gets the default model
    async fn get_default_model(&self) -> Option<ModelId>;

    /// Sets the operating model
    async fn set_default_model(&self, model_id: ModelId) -> anyhow::Result<()>;

    /// Refresh MCP caches by fetching fresh data
    async fn reload_mcp(&self) -> Result<()>;

    /// List of commands defined in .md file(s)
    async fn get_commands(&self) -> Result<Vec<Command>>;

    /// List of available skills
    async fn get_skills(&self) -> Result<Vec<Skill>>;

    /// Generate a shell command from natural language prompt
    async fn generate_command(&self, prompt: UserPrompt) -> Result<String>;

    /// Initiate provider auth flow
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> Result<AuthContextRequest>;

    /// Complete provider authentication and save credentials
    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        context: AuthContextResponse,
        timeout: std::time::Duration,
    ) -> Result<()>;

    /// Remove provider credentials (logout)
    async fn remove_provider(&self, provider_id: &ProviderId) -> Result<()>;

    /// Sync a codebase directory for semantic search
    async fn sync_codebase(
        &self,
        path: PathBuf,
        batch_size: usize,
    ) -> Result<MpscStream<Result<forge_domain::SyncProgress>>>;

    /// Query the indexed codebase
    async fn query_codebase(
        &self,
        path: PathBuf,
        params: forge_domain::SearchParams<'_>,
    ) -> Result<Vec<forge_domain::Node>>;

    /// List all workspaces
    async fn list_codebases(&self) -> Result<Vec<forge_domain::WorkspaceInfo>>;

    /// Get workspace information for a specific path
    async fn get_workspace_info(
        &self,
        path: PathBuf,
    ) -> Result<Option<forge_domain::WorkspaceInfo>>;

    /// Delete a workspace
    async fn delete_codebase(&self, workspace_id: forge_domain::WorkspaceId) -> Result<()>;

    /// Hydrates the gRPC channel
    fn hydrate_channel(&self) -> Result<()>;

    /// Check if authentication credentials exist
    async fn is_authenticated(&self) -> Result<bool>;

    /// Create new authentication credentials
    async fn create_auth_credentials(&self) -> Result<forge_domain::WorkspaceAuth>;

    /// Migrate environment variable-based credentials to file-based
    /// credentials. This is a one-time migration that runs only if the
    /// credentials file doesn't exist.
    async fn migrate_env_credentials(&self) -> Result<Option<forge_domain::MigrationResult>>;

    async fn generate_data(
        &self,
        data_parameters: DataGenerationParameters,
    ) -> Result<BoxStream<'static, Result<serde_json::Value, anyhow::Error>>>;
}
