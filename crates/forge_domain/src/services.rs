use std::path::Path;

use crate::{
    Agent, Attachment, ChatCompletionMessage, CompactionResult, Context, Conversation,
    ConversationId, Environment, File, Model, ModelId, ResultStream, ToolCallContext, ToolCallFull,
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

#[async_trait::async_trait]
pub trait WorkflowService {
    /// Find a forge.yaml config file by traversing parent directories.
    /// Returns the path to the first found config file, or the original path if
    /// none is found.
    async fn resolve(&self, path: Option<std::path::PathBuf>) -> std::path::PathBuf;

    /// Reads the workflow from the given path.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    async fn read(&self, path: Option<&Path>) -> anyhow::Result<Workflow>;

    /// Writes the given workflow to the specified path.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    async fn write(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()>;

    /// Updates the workflow at the given path using the provided closure.
    /// If no path is provided, it will try to find forge.yaml in the current
    /// directory or its parent directories.
    ///
    /// The closure receives a mutable reference to the workflow, which can be
    /// modified. After the closure completes, the updated workflow is
    /// written back to the same path.
    async fn update_workflow<F>(&self, path: Option<&Path>, f: F) -> anyhow::Result<Workflow>
    where
        F: FnOnce(&mut Workflow) + Send;
}

#[async_trait::async_trait]
pub trait SuggestionService: Send + Sync {
    async fn suggestions(&self) -> anyhow::Result<Vec<File>>;
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
    type WorkflowService: WorkflowService;
    type SuggestionService: SuggestionService;

    fn tool_service(&self) -> &Self::ToolService;
    fn provider_service(&self) -> &Self::ProviderService;
    fn conversation_service(&self) -> &Self::ConversationService;
    fn template_service(&self) -> &Self::TemplateService;
    fn attachment_service(&self) -> &Self::AttachmentService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn compaction_service(&self) -> &Self::CompactionService;
    fn workflow_service(&self) -> &Self::WorkflowService;
    fn suggestion_service(&self) -> &Self::SuggestionService;
}
