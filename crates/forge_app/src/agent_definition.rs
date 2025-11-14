use derive_setters::Setters;
use forge_domain::{
    AgentId, Compact, MaxTokens, ModelId, ProviderId, Temperature, ToolName, TopK, TopP,
};
use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::{EventContext, SystemContext, Template};

/// Agent definition for deserialization from config files
/// Used during configuration loading before defaults are resolved
#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters, JsonSchema)]
#[setters(strip_option, into)]
#[merge(strategy = merge::option::overwrite_none)]
pub struct AgentDefinition {
    /// Flag to enable/disable tool support for this agent.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub tool_supported: Option<bool>,

    // Unique identifier for the agent
    #[merge(skip)]
    pub id: AgentId,

    /// Human-readable title for the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub provider: Option<ProviderId>,

    // The language model ID to be used by this agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub model: Option<ModelId>,

    // Human-readable description of the agent's purpose
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub description: Option<String>,

    // Template for the system prompt provided to the agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub system_prompt: Option<Template<SystemContext>>,

    // Template for the user prompt provided to the agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub user_prompt: Option<Template<EventContext>>,

    /// Tools that the agent can use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = merge_opt_vec)]
    pub tools: Option<Vec<ToolName>>,

    /// Maximum number of turns the agent can take
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub max_turns: Option<u64>,

    /// Maximum depth to which the file walker should traverse for this agent
    /// If not provided, the maximum possible depth will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub max_walker_depth: Option<usize>,

    /// Configuration for automatic context compaction
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub compact: Option<Compact>,

    /// A set of custom rules that the agent should follow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub custom_rules: Option<String>,

    /// Temperature used for agent
    ///
    /// Temperature controls the randomness in the model's output.
    /// - Lower values (e.g., 0.1) make responses more focused, deterministic,
    ///   and coherent
    /// - Higher values (e.g., 0.8) make responses more creative, diverse, and
    ///   exploratory
    /// - Valid range is 0.0 to 2.0
    /// - If not specified, the model provider's default temperature will be
    ///   used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub temperature: Option<Temperature>,

    /// Top-p (nucleus sampling) used for agent
    ///
    /// Controls the diversity of the model's output by considering only the
    /// most probable tokens up to a cumulative probability threshold.
    /// - Lower values (e.g., 0.1) make responses more focused
    /// - Higher values (e.g., 0.9) make responses more diverse
    /// - Valid range is 0.0 to 1.0
    /// - If not specified, the model provider's default will be used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub top_p: Option<TopP>,

    /// Top-k used for agent
    ///
    /// Controls the number of highest probability vocabulary tokens to keep.
    /// - Lower values (e.g., 10) make responses more focused
    /// - Higher values (e.g., 100) make responses more diverse
    /// - Valid range is 1 to 1000
    /// - If not specified, the model provider's default will be used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub top_k: Option<TopK>,

    /// Maximum number of tokens the model can generate
    ///
    /// Controls the maximum length of the model's response.
    /// - Lower values (e.g., 100) limit response length for concise outputs
    /// - Higher values (e.g., 4000) allow for longer, more detailed responses
    /// - Valid range is 1 to 100,000
    /// - If not specified, the model provider's default will be used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub max_tokens: Option<MaxTokens>,

    /// Reasoning configuration for the agent.
    /// Controls the reasoning capabilities of the agent
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub reasoning: Option<forge_domain::ReasoningConfig>,
    /// Maximum number of times a tool can fail before sending the response back
    /// to the LLM forces the completion.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(skip)]
    pub max_requests_per_turn: Option<usize>,
}

fn merge_opt_vec<T>(base: &mut Option<Vec<T>>, other: Option<Vec<T>>) {
    if let Some(other) = other {
        if let Some(base) = base {
            base.extend(other);
        } else {
            *base = Some(other);
        }
    }
}

impl AgentDefinition {
    /// Creates a new agent definition with the given ID
    pub fn new(id: impl Into<AgentId>) -> Self {
        Self {
            id: id.into(),
            title: Default::default(),
            tool_supported: Default::default(),
            model: Default::default(),
            description: Default::default(),
            system_prompt: Default::default(),
            user_prompt: Default::default(),
            tools: Default::default(),
            max_turns: Default::default(),
            max_walker_depth: Default::default(),
            compact: Default::default(),
            custom_rules: Default::default(),
            temperature: Default::default(),
            top_p: Default::default(),
            top_k: Default::default(),
            max_tokens: Default::default(),
            reasoning: Default::default(),
            max_tool_failure_per_turn: Default::default(),
            max_requests_per_turn: Default::default(),
            provider: Default::default(),
        }
    }

    /// Convert AgentDefinition to Agent with required provider and model
    /// Falls back to provided defaults if not specified in definition
    pub fn into_agent(
        self,
        default_provider: ProviderId,
        default_model: ModelId,
    ) -> forge_domain::Agent {
        let provider = self.provider.unwrap_or(default_provider);
        let model = self.model.unwrap_or(default_model);

        forge_domain::Agent {
            tool_supported: self.tool_supported,
            id: self.id,
            title: self.title,
            provider,
            model,
            description: self.description,
            system_prompt: self.system_prompt,
            user_prompt: self.user_prompt,
            tools: self.tools,
            max_turns: self.max_turns,
            max_walker_depth: self.max_walker_depth,
            compact: self.compact,
            custom_rules: self.custom_rules,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            max_tokens: self.max_tokens,
            reasoning: self.reasoning,
            max_tool_failure_per_turn: self.max_tool_failure_per_turn,
            max_requests_per_turn: self.max_requests_per_turn,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_agent_definition_new() {
        let actual = AgentDefinition::new("test_agent");

        assert_eq!(actual.id.as_str(), "test_agent");
        assert!(actual.title.is_none());
        assert!(actual.tool_supported.is_none());
        assert!(actual.model.is_none());
    }

    #[test]
    fn test_into_agent_with_defaults() {
        let definition = AgentDefinition::new("test_agent");
        let actual = definition.into_agent(
            ProviderId::from("test_provider"),
            ModelId::from("test_model"),
        );

        assert_eq!(actual.id.as_str(), "test_agent");
        assert_eq!(actual.provider.as_str(), "test_provider");
        assert_eq!(actual.model.as_str(), "test_model");
    }

    #[test]
    fn test_into_agent_with_overrides() {
        let definition = AgentDefinition::new("test_agent")
            .provider(ProviderId::from("override_provider"))
            .model(ModelId::from("override_model"));

        let actual = definition.into_agent(
            ProviderId::from("default_provider"),
            ModelId::from("default_model"),
        );

        assert_eq!(actual.id.as_str(), "test_agent");
        assert_eq!(actual.provider.as_str(), "override_provider");
        assert_eq!(actual.model.as_str(), "override_model");
    }
}
