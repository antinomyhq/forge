use std::path::Path;

pub use forge_domain::*;
use forge_stream::MpscStream;
use serde_json::Value;

#[async_trait::async_trait]
pub trait API: Sync + Send {
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
