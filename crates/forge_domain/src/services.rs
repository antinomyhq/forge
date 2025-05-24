use std::path::Path;
use std::sync::Arc;

use crate::{
    Agent, Attachment, ChatCompletionMessage, CompactionResult, Context, Conversation,
    ConversationId, Environment, File, ForgeConfig, ForgeKey, InitAuth, McpConfig, Model, ModelId,
    ResultStream, Scope, Tool, ToolCallContext, ToolCallFull, ToolDefinition, ToolName, ToolResult,
    Workflow,
};

#[async_trait::async_trait]
pub trait ChatService: Send + Sync {
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
    async fn call(
        &self,
        context: ToolCallContext,
        call: ToolCallFull,
    ) -> anyhow::Result<ToolResult>;
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>>;
}

#[async_trait::async_trait]
pub trait McpConfigManager: Send + Sync {
    /// Responsible to load the MCP servers from all configuration files.
    async fn read(&self) -> anyhow::Result<McpConfig>;

    /// Responsible for writing the McpConfig on disk.
    async fn write(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait McpService: Send + Sync {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    async fn find(&self, name: &ToolName) -> anyhow::Result<Option<Arc<Tool>>>;
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

#[async_trait::async_trait]
pub trait KeyService: Send + Sync {
    async fn get(&self) -> Option<ForgeKey>;
    async fn set(&self, key: ForgeKey) -> anyhow::Result<()>;
    async fn delete(&self) -> anyhow::Result<()>;
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

#[async_trait::async_trait]
pub trait ConfigService: Send + Sync {
    async fn read(&self) -> anyhow::Result<ForgeConfig>;
    async fn write(&self, config: &ForgeConfig) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait AuthService: Send + Sync {
    async fn init(&self) -> anyhow::Result<InitAuth>;
    async fn login(&self, auth: &InitAuth) -> anyhow::Result<()>;
    async fn logout(&self) -> anyhow::Result<()>;
}

/// Core app trait providing access to services and repositories.
/// This trait follows clean architecture principles for dependency management
/// and service/repository composition.
pub trait Services: Send + Sync + 'static + Clone {
    type ToolService: ToolService;
    type ChatService: ChatService;
    type ConversationService: ConversationService;
    type TemplateService: TemplateService;
    type AttachmentService: AttachmentService;
    type EnvironmentService: EnvironmentService;
    type CompactionService: CompactionService;
    type WorkflowService: WorkflowService;
    type SuggestionService: SuggestionService;
    type McpConfigManager: McpConfigManager;
    type AuthService: AuthService;
    type ConfigService: ConfigService;
    type KeyService: KeyService;

    fn tool_service(&self) -> &Self::ToolService;
    fn chat_service(&self) -> &Self::ChatService;
    fn conversation_service(&self) -> &Self::ConversationService;
    fn template_service(&self) -> &Self::TemplateService;
    fn attachment_service(&self) -> &Self::AttachmentService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn compaction_service(&self) -> &Self::CompactionService;
    fn workflow_service(&self) -> &Self::WorkflowService;
    fn suggestion_service(&self) -> &Self::SuggestionService;
    fn mcp_config_manager(&self) -> &Self::McpConfigManager;
    fn auth_service(&self) -> &Self::AuthService;
    fn config_service(&self) -> &Self::ConfigService;
    fn key_service(&self) -> &Self::KeyService;
}
