use std::path::{Path, PathBuf};

use anyhow::Result;
use forge_app::dto::{InitAuth, ProviderId, ToolsOverview};
use forge_app::{User, UserUsage};
use forge_domain::AgentId;
use forge_stream::MpscStream;

use crate::*;

#[async_trait::async_trait]
pub trait API: Sync + Send {
    /// Provides a list of files in the current working directory for auto
    /// completion
    async fn discover(&self) -> Result<Vec<crate::File>>;

    /// Provides information about the tools available in the current
    /// environment
    async fn tools(&self) -> anyhow::Result<ToolsOverview>;

    /// Provides a list of models available in the current environment
    async fn models(&self) -> Result<Vec<Model>>;
    /// Provides a list of agents available in the current environment
    async fn get_agents(&self) -> Result<Vec<Agent>>;
    /// Provides a list of providers available in the current environment
    async fn providers(&self) -> Result<Vec<Provider>>;

    /// Executes a chat request and returns a stream of responses
    async fn chat(&self, chat: ChatRequest) -> Result<MpscStream<Result<ChatResponse>>>;

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

    /// Writes the given workflow to the specified path
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories
    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> Result<()>;

    /// Updates the workflow at the given path using the provided closure
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories
    async fn update_workflow<F>(&self, path: Option<&Path>, f: F) -> Result<Workflow>
    where
        F: FnOnce(&mut Workflow) + Send;

    /// Returns the conversation with the given ID
    async fn conversation(&self, conversation_id: &ConversationId) -> Result<Option<Conversation>>;

    /// Lists all conversations for the active workspace
    async fn list_conversations(&self, limit: Option<usize>) -> Result<Vec<Conversation>>;

    /// Finds the last active conversation for the current workspace
    async fn last_conversation(&self) -> Result<Option<Conversation>>;

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
    /// local taking precedence
    async fn read_mcp_config(&self) -> Result<McpConfig>;

    /// Writes the provided MCP configuration to disk at the specified scope
    /// The scope determines whether the configuration is written to user-level
    /// or local configuration User-level configuration is stored in the
    /// user's home directory Local configuration is stored in the current
    /// project directory
    async fn write_mcp_config(&self, scope: &Scope, config: &McpConfig) -> Result<()>;

    async fn init_login(&self) -> Result<InitAuth>;
    async fn get_login_info(&self) -> Result<Option<LoginInfo>>;
    async fn login(&self, auth: &InitAuth) -> Result<()>;
    async fn logout(&self) -> anyhow::Result<()>;
    async fn get_provider(&self) -> anyhow::Result<Provider>;
    async fn set_provider(&self, provider_id: ProviderId) -> anyhow::Result<()>;
    async fn user_info(&self) -> anyhow::Result<Option<User>>;
    async fn user_usage(&self) -> anyhow::Result<Option<UserUsage>>;

    /// Gets the currently operating agent
    async fn get_operating_agent(&self) -> Option<AgentId>;

    /// Sets the operating agent
    async fn set_operating_agent(&self, agent_id: AgentId) -> anyhow::Result<()>;
}
