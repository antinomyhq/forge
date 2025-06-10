use std::sync::Arc;
use forge_domain::{
    Agent, ChatCompletionMessage, Context, ModelId, ResultStream, ToolCallContext, ToolCallFull,
    ToolResult,
};

use crate::Services;
use crate::ProviderService;
use crate::tool_registry::ToolRegistry;

/// Agent service trait that provides core chat and tool call functionality.
/// This trait abstracts the essential operations needed by the Orchestrator.
#[async_trait::async_trait]
pub trait AgentService: Send + Sync + 'static {
    /// Execute a chat completion request
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error>;

    /// Execute a tool call
    async fn call(
        &self,
        agent: &Agent,
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult;
}

/// Blanket implementation of AgentService for any type that implements Services
#[async_trait::async_trait]
impl<T> AgentService for T
where
    T: Services,
{
    async fn chat(
        &self,
        id: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.provider_service().chat(id, context).await
    }

    async fn call(
        &self,
        _agent: &Agent,
        context: &mut ToolCallContext,
        call: ToolCallFull,
    ) -> ToolResult {
        let registry = ToolRegistry::new(Arc::new(self.clone()));
        registry.call(context, call).await
    }
}
