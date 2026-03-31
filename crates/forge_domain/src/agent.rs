use derive_setters::Setters;

use crate::{
    AgentId, Compact, Error, EventContext, MaxTokens, ModelId, ProviderId, ReasoningConfig, Result,
    SystemContext, Temperature, Template, ToolDefinition, ToolName, TopK, TopP,
};

/// Runtime agent representation with required model and provider
#[derive(Debug, Clone, PartialEq, Setters)]
#[setters(strip_option, into)]
pub struct Agent {
    /// Flag to enable/disable tool support for this agent.
    pub tool_supported: Option<bool>,

    // Unique identifier for the agent
    pub id: AgentId,

    /// Path to the agent definition file, if loaded from a file
    pub path: Option<String>,

    /// Human-readable title for the agent
    pub title: Option<String>,

    // Required provider for the agent
    pub provider: ProviderId,

    // Required language model ID to be used by this agent
    pub model: ModelId,

    // Human-readable description of the agent's purpose
    pub description: Option<String>,

    // Template for the system prompt provided to the agent
    pub system_prompt: Option<Template<SystemContext>>,

    // Template for the user prompt provided to the agent
    pub user_prompt: Option<Template<EventContext>>,

    /// Tools that the agent can use
    pub tools: Option<Vec<ToolName>>,

    /// Maximum number of turns the agent can take
    pub max_turns: Option<u64>,

    /// Configuration for automatic context compaction
    pub compact: Compact,

    /// A set of custom rules that the agent should follow
    pub custom_rules: Option<String>,

    /// Temperature used for agent
    pub temperature: Option<Temperature>,

    /// Top-p (nucleus sampling) used for agent
    pub top_p: Option<TopP>,

    /// Top-k used for agent
    pub top_k: Option<TopK>,

    /// Maximum number of tokens the model can generate
    pub max_tokens: Option<MaxTokens>,

    /// Reasoning configuration for the agent.
    pub reasoning: Option<ReasoningConfig>,

    /// Maximum number of times a tool can fail before sending the response back
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn
    pub max_requests_per_turn: Option<usize>,
}

impl Agent {
    /// Create a new Agent with required provider and model
    pub fn new(id: impl Into<AgentId>, provider: ProviderId, model: ModelId) -> Self {
        Self {
            id: id.into(),
            provider,
            model,
            title: Default::default(),
            tool_supported: Default::default(),
            description: Default::default(),
            system_prompt: Default::default(),
            user_prompt: Default::default(),
            tools: Default::default(),
            max_turns: Default::default(),
            compact: Compact::default(),
            custom_rules: Default::default(),
            temperature: Default::default(),
            top_p: Default::default(),
            top_k: Default::default(),
            max_tokens: Default::default(),
            reasoning: Default::default(),
            max_tool_failure_per_turn: Default::default(),
            max_requests_per_turn: Default::default(),
            path: Default::default(),
        }
    }

    /// Creates a ToolDefinition from this agent
    ///
    /// # Errors
    ///
    /// Returns an error if the agent has no description
    pub fn tool_definition(&self) -> Result<ToolDefinition> {
        if self.description.is_none() || self.description.as_ref().is_none_or(|d| d.is_empty()) {
            return Err(Error::MissingAgentDescription(self.id.clone()));
        }
        Ok(ToolDefinition::new(self.id.as_str().to_string())
            .description(self.description.clone().unwrap()))
    }

    /// Sets the model in compaction config if not already set
    pub fn set_compact_model_if_none(mut self) -> Self {
        if self.compact.model.is_none() {
            self.compact.model = Some(self.model.clone());
        }
        self
    }

    /// Gets the tool ordering for this agent, derived from the tools list
    pub fn tool_order(&self) -> crate::ToolOrder {
        self.tools
            .as_ref()
            .map(|tools| crate::ToolOrder::from_tool_list(tools))
            .unwrap_or_default()
    }
}

impl From<Agent> for ToolDefinition {
    fn from(value: Agent) -> Self {
        let description = value.description.unwrap_or_default();
        let name = ToolName::new(value.id);
        ToolDefinition {
            name,
            description,
            input_schema: schemars::schema_for!(crate::AgentInput),
        }
    }
}
