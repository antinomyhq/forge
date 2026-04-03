use std::sync::Arc;

use forge_config::{ForgeConfig, ModelConfig, Preset};
use forge_domain::{
    Agent, ChatCompletionMessage, Compact, Context, Conversation, Effort, MaxTokens,
    ModelId, ProviderId, ReasoningConfig, ResultStream, Temperature, ToolCallContext,
    ToolCallFull, ToolResult, TopK, TopP,
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

        // Resolve the agent-specific ModelConfig from ForgeConfig.
        let agent_model_config: Option<&ModelConfig> = match agent.id.as_str() {
            "forge" => config.agent_forge.as_ref(),
            "muse" => config.agent_muse.as_ref(),
            "sage" => config.agent_sage.as_ref(),
            _ => None,
        };

        // Apply model/provider from agent-specific config.
        if let Some(mc) = agent_model_config {
            if let Some(ref model_id) = mc.model_id {
                agent.model = ModelId::new(model_id);
            }
            if let Some(ref provider_id) = mc.provider_id {
                agent.provider = ProviderId::from(provider_id.clone());
            }
        }

        // Resolve the preset: agent-specific preset_id takes priority over
        // nothing (there is no global preset_id on ForgeConfig).
        let preset: Option<&Preset> = agent_model_config
            .and_then(|mc| mc.preset_id.as_deref())
            .and_then(|id| config.presets.get(id));

        // Helper: convert a config ReasoningConfig to a domain ReasoningConfig.
        let to_domain_reasoning =
            |r: &forge_config::ReasoningConfig| -> ReasoningConfig {
                use forge_config::Effort as ConfigEffort;
                ReasoningConfig {
                    effort: r.effort.as_ref().map(|e| match e {
                        ConfigEffort::None => Effort::None,
                        ConfigEffort::Minimal => Effort::Minimal,
                        ConfigEffort::Low => Effort::Low,
                        ConfigEffort::Medium => Effort::Medium,
                        ConfigEffort::High => Effort::High,
                        ConfigEffort::XHigh => Effort::XHigh,
                        ConfigEffort::Max => Effort::Max,
                    }),
                    max_tokens: r.max_tokens,
                    exclude: r.exclude,
                    enabled: r.enabled,
                }
            };

        // --- Apply LLM settings in priority order (lowest → highest) ---
        // 1. Config global settings
        // 2. Preset settings (from agent-specific ModelConfig's preset_id)
        // 3. Agent's own values (never overwritten)

        // temperature
        if agent.temperature.is_none() {
            let value = preset
                .map(|p| p.temperature)
                .or(config.temperature)
                .and_then(|d| Temperature::new(d.0 as f32).ok());
            if let Some(v) = value {
                agent.temperature = Some(v);
            }
        }

        // top_p
        if agent.top_p.is_none() {
            let value = preset
                .map(|p| p.top_p)
                .or(config.top_p)
                .and_then(|d| TopP::new(d.0 as f32).ok());
            if let Some(v) = value {
                agent.top_p = Some(v);
            }
        }

        // top_k
        if agent.top_k.is_none() {
            let value = preset
                .map(|p| Some(p.top_k))
                .unwrap_or(config.top_k)
                .and_then(|k| TopK::new(k).ok());
            if let Some(v) = value {
                agent.top_k = Some(v);
            }
        }

        // max_tokens
        if agent.max_tokens.is_none() {
            let value = preset
                .and_then(|p| p.max_tokens)
                .or(config.max_tokens)
                .and_then(|m| MaxTokens::new(m).ok());
            if let Some(v) = value {
                agent.max_tokens = Some(v);
            }
        }

        // tool_supported: preset > config global; agent's own value wins when set
        if agent.tool_supported.is_none() {
            let value = preset
                .map(|p| p.tool_supported)
                .unwrap_or(config.tool_supported);
            agent.tool_supported = Some(value);
        }

        // max_tool_failure_per_turn: agent's own value wins
        if agent.max_tool_failure_per_turn.is_none() {
            if let Some(v) = config.max_tool_failure_per_turn {
                agent.max_tool_failure_per_turn = Some(v);
            }
        }

        // max_requests_per_turn: agent's own value wins
        if agent.max_requests_per_turn.is_none() {
            if let Some(v) = config.max_requests_per_turn {
                agent.max_requests_per_turn = Some(v);
            }
        }

        // compact: merge workflow config into agent (agent fields take priority)
        if let Some(ref workflow_compact) = config.compact {
            let mut merged_compact = Compact {
                retention_window: workflow_compact.retention_window,
                eviction_window: workflow_compact.eviction_window.value(),
                max_tokens: workflow_compact.max_tokens,
                token_threshold: workflow_compact.token_threshold,
                turn_threshold: workflow_compact.turn_threshold,
                message_threshold: workflow_compact.message_threshold,
                model: workflow_compact.model.as_deref().map(ModelId::new),
                on_turn_end: workflow_compact.on_turn_end,
            };
            merged_compact.merge(agent.compact.clone());
            agent.compact = merged_compact;
        }

        // reasoning: preset > config global; agent fields take highest priority
        let base_reasoning = preset
            .and_then(|p| p.reasoning.as_ref())
            .or(config.reasoning.as_ref())
            .map(to_domain_reasoning);

        if let Some(base) = base_reasoning {
            let mut merged = agent.reasoning.clone().unwrap_or_default();
            merged.merge(base);
            agent.reasoning = Some(merged);
        }

        agent
    }
}

#[cfg(test)]
mod tests {
    use forge_config::{
        Decimal, Effort as ConfigEffort, ModelConfig, Preset, ReasoningConfig as ConfigReasoningConfig,
    };
    use forge_domain::{AgentId, Effort, ModelId, ProviderId, ReasoningConfig, Temperature, TopP};
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_agent() -> Agent {
        Agent::new(
            AgentId::new("test"),
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
    }

    fn fixture_forge_agent() -> Agent {
        Agent::new(
            AgentId::FORGE,
            ProviderId::ANTHROPIC,
            ModelId::new("claude-3-5-sonnet-20241022"),
        )
    }

    /// When the agent has no reasoning config, the config's reasoning is
    /// applied in full.
    #[test]
    fn test_reasoning_applied_from_config_when_agent_has_none() {
        let config = ForgeConfig::default().reasoning(
            ConfigReasoningConfig::default()
                .enabled(true)
                .effort(ConfigEffort::Medium),
        );

        let actual = fixture_agent().apply_config(&config).reasoning;

        let expected = Some(
            ReasoningConfig::default()
                .enabled(true)
                .effort(Effort::Medium),
        );

        assert_eq!(actual, expected);
    }

    /// When the agent already has reasoning fields set, those fields take
    /// priority; config only fills in fields the agent left unset.
    #[test]
    fn test_reasoning_agent_fields_take_priority_over_config() {
        let config = ForgeConfig::default().reasoning(
            ConfigReasoningConfig::default()
                .enabled(true)
                .effort(ConfigEffort::Low)
                .max_tokens(1024_usize),
        );

        // Agent overrides effort but leaves enabled and max_tokens unset.
        let agent = fixture_agent().reasoning(ReasoningConfig::default().effort(Effort::High));

        let actual = agent.apply_config(&config).reasoning;

        let expected = Some(
            ReasoningConfig::default()
                .effort(Effort::High) // agent's value wins
                .enabled(true) // filled in from config
                .max_tokens(1024_usize), // filled in from config
        );

        assert_eq!(actual, expected);
    }

    /// agent_forge config overrides model and provider on a FORGE agent.
    #[test]
    fn test_agent_specific_model_and_provider_applied() {
        let config = ForgeConfig::default().agent_forge(
            ModelConfig::default()
                .model_id("gpt-4o")
                .provider_id("openai"),
        );

        let actual = fixture_forge_agent().apply_config(&config);

        assert_eq!(actual.model, ModelId::new("gpt-4o"));
        assert_eq!(actual.provider, ProviderId::from("openai".to_string()));
    }

    /// agent_forge config does not affect a non-FORGE agent.
    #[test]
    fn test_agent_specific_config_not_applied_to_other_agents() {
        let config = ForgeConfig::default().agent_forge(
            ModelConfig::default()
                .model_id("gpt-4o")
                .provider_id("openai"),
        );

        let actual = fixture_agent().apply_config(&config);

        // Model and provider remain unchanged.
        assert_eq!(actual.model, ModelId::new("claude-3-5-sonnet-20241022"));
        assert_eq!(actual.provider, ProviderId::ANTHROPIC);
    }

    /// Preset LLM settings are applied when the agent-specific ModelConfig
    /// references a preset_id that exists in config.presets.
    #[test]
    fn test_preset_settings_applied_via_agent_model_config() {
        let mut presets = std::collections::HashMap::new();
        presets.insert(
            "fast".to_string(),
            Preset { temperature: Decimal(0.2), top_p: Decimal(0.8), ..Default::default() },
        );

        let config = ForgeConfig {
            presets,
            agent_forge: Some(ModelConfig::default().preset_id("fast")),
            ..Default::default()
        };

        let actual = fixture_forge_agent().apply_config(&config);

        assert_eq!(actual.temperature, Temperature::new(0.2).ok());
        assert_eq!(actual.top_p, TopP::new(0.8).ok());
    }

    /// Preset settings take priority over config global settings.
    #[test]
    fn test_preset_takes_priority_over_global_config() {
        let mut presets = std::collections::HashMap::new();
        presets.insert(
            "precise".to_string(),
            Preset { temperature: Decimal(0.1), ..Default::default() },
        );

        let config = ForgeConfig {
            presets,
            // Global temperature is higher; preset should win.
            temperature: Some(Decimal(1.0)),
            agent_forge: Some(ModelConfig::default().preset_id("precise")),
            ..Default::default()
        };

        let actual = fixture_forge_agent().apply_config(&config);

        assert_eq!(actual.temperature, Temperature::new(0.1).ok());
    }

    /// Agent's own temperature takes priority over both preset and global config.
    #[test]
    fn test_agent_temperature_takes_priority_over_preset_and_global() {
        let mut presets = std::collections::HashMap::new();
        presets.insert(
            "fast".to_string(),
            Preset { temperature: Decimal(0.2), ..Default::default() },
        );

        let config = ForgeConfig {
            presets,
            temperature: Some(Decimal(1.0)),
            agent_forge: Some(ModelConfig::default().preset_id("fast")),
            ..Default::default()
        };

        let agent =
            fixture_forge_agent().temperature(Temperature::new(0.5).unwrap());

        let actual = agent.apply_config(&config);

        assert_eq!(actual.temperature, Temperature::new(0.5).ok());
    }
}
