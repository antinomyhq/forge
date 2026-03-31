use fake::Dummy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Effort level for the reasoning capability.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Dummy)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    /// No reasoning tokens; disables extended thinking entirely.
    None,
    /// Minimal reasoning; fastest, fewest thinking tokens.
    Minimal,
    /// Constrained reasoning suitable for straightforward tasks.
    Low,
    /// Balanced reasoning for moderately complex tasks.
    Medium,
    /// Deep reasoning for complex problems.
    High,
    /// Maximum reasoning budget for the hardest tasks.
    #[serde(rename = "xhigh")]
    XHigh,
}

/// Reasoning configuration applied to all agents when set at the global level.
///
/// Controls the reasoning capabilities of the model. When set here, it acts as
/// a default for all agents; agent-level settings take priority over this
/// global setting.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Dummy)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningConfig {
    /// Controls the effort level of the agent's reasoning.
    /// Supported by openrouter and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    /// Controls how many tokens the model can spend thinking.
    /// Supported by openrouter, anthropic and forge provider.
    /// Should be greater than 1024 but less than overall max_tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// Model thinks deeply, but the reasoning is hidden from you.
    /// Supported by openrouter and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,

    /// Enables reasoning at the "medium" effort level with no exclusions.
    /// Supported by openrouter, anthropic and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
