use std::sync::Arc;

use forge_domain::{
    Agent, ChatCompletionMessage, Context, Conversation, ModelId, ProviderId, ResultStream,
    Template, ToolCallContext, ToolCallFull, ToolResult,
};

use crate::tool_registry::ToolRegistry;
use crate::{ConversationService, ProviderRegistry, ProviderService, Services, TemplateService};

/// Agent service trait that provides core chat and tool call functionality.
/// This trait abstracts the essential operations needed by the Orchestrator.
#[async_trait::async_trait]
pub trait AgentService: Send + Sync + 'static {
    /// Execute a chat completion request
    async fn chat_agent(
        &self,
        id: &ModelId,
        context: Context,
        provider_id: Option<ProviderId>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;

    /// Execute a tool call
    async fn call(
        &self,
        agent: &Agent,
        context: &ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;

    /// Render a template with the provided object
    async fn render<V: serde::Serialize + Send + Sync>(
        &self,
        template: Template<V>,
        object: &V,
    ) -> anyhow::Result<String>;

    /// Synchronize the on-going conversation
    async fn update(&self, conversation: Conversation) -> anyhow::Result<()>;
}

/// Blanket implementation of AgentService for any type that implements Services
#[async_trait::async_trait]
impl<T: Services> AgentService for T {
    async fn chat_agent(
        &self,
        id: &ModelId,
        context: Context,
        provider_id: Option<ProviderId>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let provider = if let Some(provider_id) = provider_id {
            self.get_provider(provider_id).await?
        } else {
            self.get_default_provider().await?
        };

        self.chat(id, context, provider).await
    }

    async fn call(
        &self,
        agent: &Agent,
        context: &ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult {
        let registry = ToolRegistry::new(Arc::new(self.clone()));
        registry.call(agent, context, call).await
    }

    async fn render<V: serde::Serialize + Send + Sync>(
        &self,
        template: Template<V>,
        object: &V,
    ) -> anyhow::Result<String> {
        self.render_template(template, object).await
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.upsert_conversation(conversation).await
    }
}
