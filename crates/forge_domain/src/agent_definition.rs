use std::borrow::Cow;

use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::Display as StrumDisplay;

// Unique identifier for an agent
#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AgentId(Cow<'static, str>);

impl From<&str> for AgentId {
    fn from(value: &str) -> Self {
        AgentId(Cow::Owned(value.to_string()))
    }
}

impl AgentId {
    // Creates a new agent ID from a string-like value
    pub fn new(id: impl ToString) -> Self {
        Self(Cow::Owned(id.to_string()))
    }

    // Returns the agent ID as a string reference
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub const FORGE: AgentId = AgentId(Cow::Borrowed("forge"));
    pub const MUSE: AgentId = AgentId(Cow::Borrowed("muse"));
    pub const SAGE: AgentId = AgentId(Cow::Borrowed("sage"));
}

impl Default for AgentId {
    fn default() -> Self {
        AgentId::FORGE
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters, JsonSchema, PartialEq)]
#[setters(strip_option)]
#[merge(strategy = merge::option::overwrite_none)]
pub struct ReasoningConfig {
    /// Controls the effort level of the agent's reasoning
    /// supported by openrouter and forge provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    /// Controls how many tokens the model can spend thinking.
    /// supported by openrouter, anthropic and forge provider
    /// should be greater then 1024 but less than overall max_tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// Model thinks deeply, but the reasoning is hidden from you.
    /// supported by openrouter and forge provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,

    /// Enables reasoning at the "medium" effort level with no exclusions.
    /// supported by openrouter, anthropic and forge provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, StrumDisplay)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Effort {
    High,
    Medium,
    Low,
}

/// Converts a thinking budget (max_tokens) to Effort
/// - 0-1024 → Low
/// - 1025-8192 → Medium
/// - 8193+ → High
impl From<usize> for Effort {
    fn from(budget: usize) -> Self {
        if budget <= 1024 {
            Effort::Low
        } else if budget <= 8192 {
            Effort::Medium
        } else {
            Effort::High
        }
    }
}

/// Estimates the token count from a string representation
/// This is a simple estimation that should be replaced with a more accurate
/// tokenizer
/// Estimates token count from a string representation
/// Re-exported for compaction reporting
pub fn estimate_token_count(count: usize) -> usize {
    // A very rough estimation that assumes ~4 characters per token on average
    // In a real implementation, this should use a proper LLM-specific tokenizer
    count / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effort_from_budget_low() {
        assert_eq!(Effort::from(0), Effort::Low);
        assert_eq!(Effort::from(1), Effort::Low);
        assert_eq!(Effort::from(1024), Effort::Low);
    }

    #[test]
    fn test_effort_from_budget_medium() {
        assert_eq!(Effort::from(1025), Effort::Medium);
        assert_eq!(Effort::from(5000), Effort::Medium);
        assert_eq!(Effort::from(8192), Effort::Medium);
    }

    #[test]
    fn test_effort_from_budget_high() {
        assert_eq!(Effort::from(8193), Effort::High);
        assert_eq!(Effort::from(10000), Effort::High);
        assert_eq!(Effort::from(100000), Effort::High);
    }
}
