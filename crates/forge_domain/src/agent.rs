use derive_setters::Setters;
use merge::Merge;

use crate::{
    AgentDefinition, AgentId, Compact, Context, Error, EventContext, MaxTokens, ModelId,
    ProviderId, ReasoningConfig, Result, SystemContext, Temperature, Template, ToolDefinition,
    ToolName, TopK, TopP, Workflow,
};

/// Runtime agent representation with required model and provider
/// Created by converting AgentDefinition with resolved defaults
#[derive(Debug, Clone, Setters)]
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
    pub compact: Option<Compact>,

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
            compact: Default::default(),
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

    /// Checks if compaction should be applied
    pub fn should_compact(&self, context: &Context, token_count: usize) -> bool {
        // Return false if compaction is not configured
        if let Some(compact) = &self.compact {
            compact.should_compact(context, token_count)
        } else {
            false
        }
    }

    /// Helper to prepare agents with workflow settings
    pub fn apply_workflow_config(self, workflow: &Workflow) -> Agent {
        let mut agent = self;
        if let Some(custom_rules) = workflow.custom_rules.clone() {
            if let Some(existing_rules) = &agent.custom_rules {
                agent.custom_rules = Some(existing_rules.clone() + "\n\n" + &custom_rules);
            } else {
                agent.custom_rules = Some(custom_rules);
            }
        }

        if let Some(temperature) = workflow.temperature {
            agent.temperature = Some(temperature);
        }

        if let Some(top_p) = workflow.top_p {
            agent.top_p = Some(top_p);
        }

        if let Some(top_k) = workflow.top_k {
            agent.top_k = Some(top_k);
        }

        if let Some(max_tokens) = workflow.max_tokens {
            agent.max_tokens = Some(max_tokens);
        }

        if let Some(tool_supported) = workflow.tool_supported {
            agent.tool_supported = Some(tool_supported);
        }
        if agent.max_tool_failure_per_turn.is_none()
            && let Some(max_tool_failure_per_turn) = workflow.max_tool_failure_per_turn
        {
            agent.max_tool_failure_per_turn = Some(max_tool_failure_per_turn);
        }

        if agent.max_requests_per_turn.is_none()
            && let Some(max_requests_per_turn) = workflow.max_requests_per_turn
        {
            agent.max_requests_per_turn = Some(max_requests_per_turn);
        }

        // Apply workflow compact configuration to agents
        if let Some(ref workflow_compact) = workflow.compact {
            if let Some(ref mut agent_compact) = agent.compact {
                // If agent already has compact config, merge workflow config into agent config
                // Agent settings take priority over workflow settings
                let mut merged_compact = workflow_compact.clone();
                merged_compact.merge(agent_compact.clone());
                *agent_compact = merged_compact;
            } else {
                // If agent doesn't have compact config, use workflow's compact config
                agent.compact = Some(workflow_compact.clone());
            }
        }

        agent
    }

    /// Sets the model in compaction config if not already set
    pub fn set_compact_model_if_none(mut self) -> Self {
        if let Some(ref mut compact) = self.compact
            && compact.model.is_none()
        {
            compact.model = Some(self.model.clone());
        }
        self
    }

    /// Converts an AgentDefinition into an Agent with resolved model and
    /// provider
    ///
    /// # Arguments
    ///
    /// * `def` - The agent definition to convert
    /// * `provider_id` - The provider ID to use if not specified in the
    ///   definition
    /// * `model_id` - The model ID to use if not specified in the definition
    pub fn from_agent_def(
        def: AgentDefinition,
        provider_id: ProviderId,
        model_id: ModelId,
    ) -> Self {
        Agent {
            tool_supported: def.tool_supported,
            id: def.id,
            title: def.title,
            description: def.description,
            provider: def.provider.unwrap_or(provider_id),
            model: def.model.unwrap_or(model_id),
            system_prompt: def.system_prompt,
            user_prompt: def.user_prompt,
            temperature: def.temperature,
            max_tokens: def.max_tokens,
            top_p: def.top_p,
            top_k: def.top_k,
            tools: def.tools,
            reasoning: def.reasoning,
            compact: def.compact,
            max_turns: def.max_turns,
            custom_rules: def.custom_rules,
            max_tool_failure_per_turn: def.max_tool_failure_per_turn,
            max_requests_per_turn: def.max_requests_per_turn,
            path: def.path,
        }
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
