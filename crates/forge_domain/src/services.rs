use serde_json::Value;

use crate::{
    Agent, Attachment, ChatCompletionMessage, CompactionResult, Context, Conversation,
    ConversationId, Environment, Model, ModelId, ResultStream, ToolCallContext, ToolCallFull,
    ToolDefinition, ToolResult, Workflow,
};

#[async_trait::async_trait]
pub trait ProviderService: Send + Sync + 'static {
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;
    async fn models(&self) -> anyhow::Result<Vec<Model>>;
}

#[async_trait::async_trait]
pub trait ToolService: Send + Sync {
    // TODO: should take `call` by reference
    async fn call(&self, context: ToolCallContext, call: ToolCallFull) -> ToolResult;
    fn list(&self) -> Vec<ToolDefinition>;
    fn usage_prompt(&self) -> String;
}

#[async_trait::async_trait]
pub trait CompactionService: Send + Sync {
    async fn compact_context(&self, agent: &Agent, context: Context) -> anyhow::Result<Context>;
}

#[async_trait::async_trait]
pub trait ConversationService: Send + Sync {
    async fn find(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>>;

    async fn upsert(&self, conversation: Conversation) -> anyhow::Result<()>;

    async fn create(&self, workflow: Workflow) -> anyhow::Result<Conversation>;

    async fn get_variable(&self, id: &ConversationId, key: &str) -> anyhow::Result<Option<Value>>;

    async fn set_variable(
        &self,
        id: &ConversationId,
        key: String,
        value: Value,
    ) -> anyhow::Result<()>;
    async fn delete_variable(&self, id: &ConversationId, key: &str) -> anyhow::Result<bool>;

    /// This is useful when you want to perform several operations on a
    /// conversation atomically.
    async fn update<F, T>(&self, id: &ConversationId, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send;

    /// Compacts the context of the main agent for the given conversation and
    /// persists it. Returns metrics about the compaction (original vs.
    /// compacted tokens and messages).
    async fn compact_conversation(&self, id: &ConversationId) -> anyhow::Result<CompactionResult>;
}

#[async_trait::async_trait]
pub trait TemplateService: Send + Sync {
    fn render(
        &self,
        template: impl ToString,
        object: &impl serde::Serialize,
    ) -> anyhow::Result<String>;
}

#[async_trait::async_trait]
pub trait AttachmentService {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>>;
}

pub trait EnvironmentService: Send + Sync {
    fn get_environment(&self) -> Environment;
}

/// Core app trait providing access to services and repositories.
/// This trait follows clean architecture principles for dependency management
/// and service/repository composition.
pub trait Services: Send + Sync + 'static + Clone {
    type ToolService: ToolService;
    type ProviderService: ProviderService;
    type ConversationService: ConversationService;
    type TemplateService: TemplateService;
    type AttachmentService: AttachmentService;
    type EnvironmentService: EnvironmentService;
    type CompactionService: CompactionService;

    fn tool_service(&self) -> &Self::ToolService;
    fn provider_service(&self) -> &Self::ProviderService;
    fn conversation_service(&self) -> &Self::ConversationService;
    fn template_service(&self) -> &Self::TemplateService;
    fn attachment_service(&self) -> &Self::AttachmentService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn compaction_service(&self) -> &Self::CompactionService;
}
