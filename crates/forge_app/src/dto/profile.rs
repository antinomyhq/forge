use derive_setters::Setters;
use forge_domain::{
    Compact, MaxTokens, ModelId, Provider, Temperature, TopK, TopP, Update, Workflow,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileName(pub String);

impl From<String> for ProfileName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<&str> for ProfileName {
    fn from(name: &str) -> Self {
        Self(name.to_string())
    }
}

impl AsRef<str> for ProfileName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Setters, Serialize, Deserialize)]
#[setters(strip_option, into)]
pub struct Profile {
    /// Unique name identifier for this profile
    /// Used to distinguish between different profile configurations
    pub name: ProfileName,

    /// AI provider configuration to use for this profile
    /// Determines which AI service (e.g., OpenAI, Anthropic, etc.) will be used
    #[serde(skip)]
    pub provider: Provider,

    // Fields from Workflow (excluding agents)
    /// Path pattern for custom template files (supports glob patterns)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub templates: Option<String>,

    /// configurations that can be used to update forge
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<Update>,

    /// Default model ID to use for agents in this workflow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,

    /// Maximum depth to which the file walker should traverse for all agents
    /// If not provided, each agent's individual setting will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_walker_depth: Option<usize>,

    /// A set of custom rules that all agents should follow
    /// These rules will be applied in addition to each agent's individual rules
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_rules: Option<String>,

    /// Temperature used for all agents
    ///
    /// Temperature controls the randomness in the model's output.
    /// - Lower values (e.g., 0.1) make responses more focused, deterministic,
    ///   and coherent
    /// - Higher values (e.g., 0.8) make responses more creative, diverse, and
    ///   exploratory
    /// - Valid range is 0.0 to 2.0
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,

    /// Top-p (nucleus sampling) used for all agents
    ///
    /// Controls the diversity of the model's output by considering only the
    /// most probable tokens up to a cumulative probability threshold.
    /// - Lower values (e.g., 0.1) make responses more focused
    /// - Higher values (e.g., 0.9) make responses more diverse
    /// - Valid range is 0.0 to 1.0
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<TopP>,

    /// Top-k used for all agents
    ///
    /// Controls the number of highest probability vocabulary tokens to keep.
    /// - Lower values (e.g., 10) make responses more focused
    /// - Higher values (e.g., 100) make responses more diverse
    /// - Valid range is 1 to 1000
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<TopK>,

    /// Maximum number of tokens the model can generate for all agents
    ///
    /// Controls the maximum length of the model's response.
    /// - Lower values (e.g., 100) limit response length for concise outputs
    /// - Higher values (e.g., 4000) allow for longer, more detailed responses
    /// - Valid range is 1 to 100,000
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<MaxTokens>,

    /// Flag to enable/disable tool support for all agents in this workflow.
    /// If not specified, each agent's individual setting will be used.
    /// Default is false (tools disabled) when not specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_supported: Option<bool>,

    /// Maximum number of times a tool can fail before the orchestrator
    /// forces the completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests_per_turn: Option<usize>,

    /// Configuration for automatic context compaction for all agents
    /// If specified, this will be applied to all agents in the workflow
    /// If not specified, each agent's individual setting will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<Compact>,
}

impl Profile {
    pub fn new(name: impl Into<ProfileName>) -> Self {
        Self {
            name: name.into(),
            provider: Provider::default(),
            templates: Default::default(),
            updates: Default::default(),
            model: Default::default(),
            max_walker_depth: Default::default(),
            custom_rules: Default::default(),
            temperature: Default::default(),
            top_p: Default::default(),
            top_k: Default::default(),
            max_tokens: Default::default(),
            tool_supported: Default::default(),
            max_tool_failure_per_turn: Default::default(),
            max_requests_per_turn: Default::default(),
            compact: Default::default(),
        }
    }

    pub fn to_workflow(&self) -> anyhow::Result<Workflow> {
        Ok(Workflow {
            templates: self.templates.clone(),
            updates: self.updates.clone(),
            model: self.model.clone(),
            max_walker_depth: self.max_walker_depth,
            custom_rules: self.custom_rules.clone(),
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            max_tokens: self.max_tokens,
            tool_supported: self.tool_supported,
            max_tool_failure_per_turn: self.max_tool_failure_per_turn,
            max_requests_per_turn: self.max_requests_per_turn,
            compact: self.compact.clone(),
            // Agents and commands are not part of a profile, so they are empty
            agents: Vec::new(),
            commands: Vec::new(),
        })
    }
}
