use std::sync::Arc;

use forge_domain::{
    Agent, ChatCompletionMessage, Context, Conversation, ModelId, ResultStream, ToolCallContext,
    ToolCallFull, ToolResult,
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
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;

    /// Execute a tool call
    async fn call(
        &self,
        agent: &Agent,
        context: &ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;

    /// Render a template with the provided object
    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
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
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let provider = self.get_active_provider().await?;
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

    async fn render(
        &self,
        template: &str,
        object: &(impl serde::Serialize + Sync),
    ) -> anyhow::Result<String> {
        self.render_template(template, object).await
    }

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.upsert_conversation(conversation).await
    }
}

/// Helper function to resolve provider for an agent
/// Tries agent-specific provider first, falls back to global provider
pub async fn resolve_provider_for_agent<T: Services>(
    services: &T,
    agent: &Agent,
) -> anyhow::Result<crate::dto::Provider> {
    if let Some(agent_provider_id) = &agent.provider {
        match services.provider_from_id(*agent_provider_id).await {
            Ok(provider) => {
                tracing::debug!(
                    agent_id = %agent.id,
                    provider_id = %agent_provider_id,
                    "Using agent-specific provider"
                );
                Ok(provider)
            }
            Err(e) => {
                tracing::warn!(
                    agent_id = %agent.id,
                    provider_id = %agent_provider_id,
                    error = %e,
                    "Agent-specific provider not available, falling back to global provider"
                );
                services.get_active_provider().await
            }
        }
    } else {
        services.get_active_provider().await
    }
}
