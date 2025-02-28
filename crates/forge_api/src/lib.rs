mod api;
mod executor;
mod loader;
mod suggestion;

use std::path::Path;

pub use api::*;
pub use forge_domain::*;
use forge_stream::MpscStream;

#[async_trait::async_trait]
pub trait API {
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

    /// Dispatches a custom event with the given name and value.
    ///
    /// # Arguments
    /// * `event_name` - The name of the event to dispatch
    /// * `event_value` - The value to pass with the event
    ///
    /// # Returns
    /// * `Ok(())` - Event was successfully dispatched
    /// * `Err` - Failed to dispatch event
    async fn dispatch(&self, event_name: &str, event_value: &str) -> anyhow::Result<()>;
}
