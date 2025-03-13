mod api;
mod executor;
mod forge_default;
mod loader;
mod suggestion;

use std::path::Path;

pub use api::*;
pub use forge_default::{create_default_workflow, DEFAULT_FORGE_YAML};
pub use forge_domain::*;
use forge_stream::MpscStream;
use serde_json::Value;

#[async_trait::async_trait]
pub trait API: Sync + Send {
    /// List snapshots for a file path
    async fn list_snapshots(
        &self,
        path: Option<&Path>,
    ) -> anyhow::Result<Vec<forge_snaps::SnapshotInfo>>;

    /// Restore a file from a snapshot by timestamp
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: &str) -> anyhow::Result<()>;

    /// Restore a file from a snapshot by index
    async fn restore_by_index(&self, file_path: &Path, index: isize) -> anyhow::Result<()>;

    /// Restore a file from its previous snapshot
    async fn restore_previous(&self, file_path: &Path) -> anyhow::Result<()>;

    /// Get a snapshot by timestamp
    async fn get_snapshot_by_timestamp(
        &self,
        file_path: &Path,
        timestamp: &str,
    ) -> anyhow::Result<forge_snaps::SnapshotMetadata>;

    /// Get a snapshot by index
    async fn get_snapshot_by_index(
        &self,
        file_path: &Path,
        index: isize,
    ) -> anyhow::Result<forge_snaps::SnapshotMetadata>;

    /// Purge snapshots older than specified days
    async fn purge_older_than(&self, days: u32) -> anyhow::Result<usize>;

    /// Provides a list of files in the current working directory for auto
    /// completion
    async fn suggestions(&self) -> anyhow::Result<Vec<File>>;

    /// Provides information about the tools available in the current
    /// environment
    async fn tools(&self) -> Vec<ToolDefinition>;

    /// Provides a list of models available in the current environment
    async fn models(&self) -> anyhow::Result<Vec<Model>>;

    /// Executes a chat request and returns a stream of responses
    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<anyhow::Result<AgentMessage<ChatResponse>, anyhow::Error>>>;

    /// Returns the current environment
    fn environment(&self) -> Environment;

    /// Creates a new conversation with the given workflow
    async fn init(&self, workflow: Workflow) -> anyhow::Result<ConversationId>;

    /// Loads a workflow configuration from the given path, current directory's
    /// forge.yaml, or embedded default configuration in that order of
    /// precedence
    async fn load(&self, path: Option<&Path>) -> anyhow::Result<Workflow>;

    /// Returns the conversation with the given ID
    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>>;

    /// Gets a variable from the conversation
    async fn get_variable(
        &self,
        conversation_id: &ConversationId,
        key: &str,
    ) -> anyhow::Result<Option<Value>>;

    /// Sets a variable in the conversation
    async fn set_variable(
        &self,
        conversation_id: &ConversationId,
        key: String,
        value: Value,
    ) -> anyhow::Result<()>;
}
