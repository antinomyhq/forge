use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::Display as StrumDisplay;

/// Controls the reasoning behaviour of a model, including effort level, token
/// budget, and visibility of the thinking process.
#[derive(
    Default, Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, fake::Dummy, Setters,
)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option)]
pub struct ReasoningConfig {
    /// Controls the effort level of the model's reasoning.
    /// Supported by openrouter and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    /// Controls how many tokens the model can spend thinking.
    /// Should be greater than 1024 but less than the overall max_tokens.
    /// Supported by openrouter, anthropic, and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// When true, the model thinks deeply but the reasoning is hidden from the
    /// caller. Supported by openrouter and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,

    /// Enables reasoning at the "medium" effort level with no exclusions.
    /// Supported by openrouter, anthropic, and forge provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Effort level for model reasoning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, fake::Dummy, StrumDisplay)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Effort {
    High,
    Medium,
    Low,
}

/// Converts a thinking budget (token count) to an [`Effort`] level.
///
/// - 0–1024 → Low
/// - 1025–8192 → Medium
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

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
