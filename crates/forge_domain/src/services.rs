use std::collections::HashMap;
use std::sync::Arc;
use rmcp::model::{CallToolResult, InitializeRequestParam};
use rmcp::{RoleClient, Service};
use rmcp::service::{RunningService, ServiceRole};

use serde_json::Value;

use crate::{Agent, Attachment, ChatCompletionMessage, Compact, Context, Conversation, ConversationId, Environment, Event, EventContext, McpConfig, McpFsServerConfig, McpHttpServerConfig, Model, ModelId, ResultStream, SystemContext, Template, ToolCallFull, ToolDefinition, ToolResult, Workflow};

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
    async fn call(&self, call: ToolCallFull) -> ToolResult;
    fn list(&self) -> Vec<ToolDefinition>;
    fn usage_prompt(&self) -> String;
}

#[async_trait::async_trait]
pub trait ConversationService: Send + Sync {
    async fn find(&self, id: &ConversationId) -> anyhow::Result<Option<Conversation>>;

    async fn upsert(&self, conversation: Conversation) -> anyhow::Result<()>;

    async fn create(&self, workflow: Workflow) -> anyhow::Result<ConversationId>;

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
}

#[async_trait::async_trait]
pub trait TemplateService: Send + Sync {
    async fn render_system(
        &self,
        agent: &Agent,
        prompt: &Template<SystemContext>,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<String>;

    async fn render_event(
        &self,
        agent: &Agent,
        prompt: &Template<EventContext>,
        event: &Event,
        variables: &HashMap<String, Value>,
    ) -> anyhow::Result<String>;

    /// Renders a custom summarization prompt for context compaction
    /// This takes a raw string template and renders it with information about
    /// the compaction and the original context (which allows for more
    /// sophisticated compaction templates)
    async fn render_summarization(
        &self,
        compaction: &Compact,
        context: &Context,
    ) -> anyhow::Result<String>;
}

#[async_trait::async_trait]
pub trait AttachmentService {
    async fn attachments(&self, url: &str) -> anyhow::Result<Vec<Attachment>>;
}

pub trait EnvironmentService: Send + Sync {
    fn get_environment(&self) -> Environment;
}

pub enum RunnableService {
    Http(RunningService<RoleClient, InitializeRequestParam>),
    Fs(RunningService<RoleClient, ()>),
}

#[async_trait::async_trait]
pub trait McpService: Send + Sync {
    async fn init_mcp(&self, config: McpConfig) -> anyhow::Result<()>;
    
    /// List tools
    async fn list_tools(&self) -> anyhow::Result<Vec<ToolDefinition>>;
    
    /// Check if an MCP server is running
    async fn is_server_running(&self, server_name: &str) -> anyhow::Result<bool>;
    
    /// Start a specific MCP server
    async fn start_http_server(&self, server_name: &str, config: McpHttpServerConfig) -> anyhow::Result<()>;
    
    /// Stop a specific MCP server
    async fn stop_server(&self, server_name: &str) -> anyhow::Result<()>;
    
    /// Stop all MCP servers
    async fn stop_all_servers(&self) -> anyhow::Result<()>;
    
    /// Get server
    async fn get_service(&self, tool_name: &str) -> anyhow::Result<Arc<RunnableService>>;
    
    /// Call tool
    async fn call_tool(&self, tool_name: &str, arguments: Value) -> anyhow::Result<CallToolResult>;
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
    type McpService: McpService;

    fn tool_service(&self) -> &Self::ToolService;
    fn provider_service(&self) -> &Self::ProviderService;
    fn conversation_service(&self) -> &Self::ConversationService;
    fn template_service(&self) -> &Self::TemplateService;
    fn attachment_service(&self) -> &Self::AttachmentService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn mcp_service(&self) -> &Self::McpService;
}
