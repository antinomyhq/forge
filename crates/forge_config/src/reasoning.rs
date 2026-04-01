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
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, fake::Dummy, StrumDisplay,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Effort {
    /// No reasoning; skips the thinking step entirely.
    None,
    /// Minimal reasoning; fastest and cheapest.
    Minimal,
    /// Low reasoning effort.
    Low,
    /// Medium reasoning effort; the default for most providers.
    Medium,
    /// High reasoning effort.
    High,
    /// Extra-high reasoning effort (OpenAI / OpenRouter).
    XHigh,
    /// Maximum reasoning effort; only available on select Anthropic models.
    Max,
}

/// Converts a thinking budget (token count) to the closest [`Effort`] level.
///
/// - 0 → None
/// - 1–512 → Minimal
/// - 513–1024 → Low
/// - 1025–8192 → Medium
/// - 8193–32768 → High
/// - 32769–65536 → XHigh
/// - 65537+ → Max
impl From<usize> for Effort {
    fn from(budget: usize) -> Self {
        if budget == 0 {
            Effort::None
        } else if budget <= 512 {
            Effort::Minimal
        } else if budget <= 1024 {
            Effort::Low
        } else if budget <= 8192 {
            Effort::Medium
        } else if budget <= 32768 {
            Effort::High
        } else if budget <= 65536 {
            Effort::XHigh
        } else {
            Effort::Max
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_effort_from_budget_none() {
        assert_eq!(Effort::from(0), Effort::None);
    }

    #[test]
    fn test_effort_from_budget_minimal() {
        assert_eq!(Effort::from(1), Effort::Minimal);
        assert_eq!(Effort::from(512), Effort::Minimal);
    }

    #[test]
    fn test_effort_from_budget_low() {
        assert_eq!(Effort::from(513), Effort::Low);
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
        assert_eq!(Effort::from(20000), Effort::High);
        assert_eq!(Effort::from(32768), Effort::High);
    }

    #[test]
    fn test_effort_from_budget_xhigh() {
        assert_eq!(Effort::from(32769), Effort::XHigh);
        assert_eq!(Effort::from(50000), Effort::XHigh);
        assert_eq!(Effort::from(65536), Effort::XHigh);
    }

    #[test]
    fn test_effort_from_budget_max() {
        assert_eq!(Effort::from(65537), Effort::Max);
        assert_eq!(Effort::from(100000), Effort::Max);
    }
}
