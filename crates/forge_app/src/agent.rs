use std::sync::Arc;

use forge_config::ForgeConfig;
use forge_domain::{
    Agent, ChatCompletionMessage, Compact, Context, Conversation, MaxTokens, ModelId, ProviderId,
    ResultStream, Temperature, ToolCallContext, ToolCallFull, ToolResult, TopK, TopP,
};
use merge::Merge;

use crate::services::AppConfigService;
use crate::tool_registry::ToolRegistry;
use crate::{ConversationService, ProviderService, Services};

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
        let provider_id = if let Some(provider_id) = provider_id {
            provider_id
        } else {
            self.get_default_provider().await?
        };
        let provider = self.get_provider(provider_id).await?;

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

    async fn update(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.upsert_conversation(conversation).await
    }
}

/// Extension trait for applying workflow-level configuration overrides to an
/// [`Agent`].
///
/// This lives in the application layer because the configuration is built
/// from [`ForgeConfig`] and applied to domain agents at runtime.
pub trait AgentExt {
    /// Applies workflow-level configuration overrides to this agent.
    ///
    /// Fields in `config` always win over agent defaults, except for
    /// `max_tool_failure_per_turn` and `max_requests_per_turn` where the
    /// agent's own value takes priority (i.e. the workflow value is only
    /// applied when the agent has no value set).
    ///
    /// # Arguments
    /// * `config` - The top-level Forge configuration.
    fn apply_config(self, config: &ForgeConfig) -> Agent;
}

impl AgentExt for Agent {
    fn apply_config(self, config: &ForgeConfig) -> Agent {
        let mut agent = self;

        if let Some(temperature) = config.temperature.and_then(|d| Temperature::new(d.0 as f32).ok()) {
            agent.temperature = Some(temperature);
        }

        if let Some(top_p) = config.top_p.and_then(|d| TopP::new(d.0 as f32).ok()) {
            agent.top_p = Some(top_p);
        }

        if let Some(top_k) = config.top_k.and_then(|k| TopK::new(k).ok()) {
            agent.top_k = Some(top_k);
        }

        if let Some(max_tokens) = config.max_tokens.and_then(|m| MaxTokens::new(m).ok()) {
            agent.max_tokens = Some(max_tokens);
        }

        if agent.max_tool_failure_per_turn.is_none()
            && let Some(max_tool_failure_per_turn) = config.max_tool_failure_per_turn
        {
            agent.max_tool_failure_per_turn = Some(max_tool_failure_per_turn);
        }

        agent.tool_supported = Some(config.tool_supported);

        if agent.max_requests_per_turn.is_none()
            && let Some(max_requests_per_turn) = config.max_requests_per_turn
        {
            agent.max_requests_per_turn = Some(max_requests_per_turn);
        }

        // Apply workflow compact configuration to agents
        if let Some(ref workflow_compact) = config.compact {
            // Merge workflow config into agent config
            // Agent settings take priority over workflow settings
            let mut merged_compact = compact_to_domain(workflow_compact);
            merged_compact.merge(agent.compact.clone());
            agent.compact = merged_compact;
        }

        agent
    }
}

/// Converts a [`forge_config::Compact`] to a [`forge_domain::Compact`].
fn compact_to_domain(c: &forge_config::Compact) -> Compact {
    Compact {
        retention_window: c.retention_window,
        eviction_window: c.eviction_window.value(),
        max_tokens: c.max_tokens,
        token_threshold: c.token_threshold,
        turn_threshold: c.turn_threshold,
        message_threshold: c.message_threshold,
        model: c.model.as_ref().map(ModelId::new),
        on_turn_end: c.on_turn_end,
    }
}
