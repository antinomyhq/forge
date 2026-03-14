use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Controls the reasoning effort level for models that support variable thinking.
///
/// This parameter is sent directly as `reasoning_effort` in the OpenAI-compatible
/// API request body. Models like GPT-5.x use this to control how much computation
/// is spent on reasoning:
/// - `low` — minimal reasoning, fastest responses
/// - `medium` — balanced reasoning effort
/// - `high` — maximum reasoning, most thorough responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffortLevel {
    Low,
    Medium,
    High,
}

impl ReasoningEffortLevel {
    /// Returns the string representation used in API requests
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasoningEffortLevel::Low => "low",
            ReasoningEffortLevel::Medium => "medium",
            ReasoningEffortLevel::High => "high",
        }
    }
}

impl fmt::Display for ReasoningEffortLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_reasoning_effort_level_serialization() {
        assert_eq!(
            serde_json::to_value(ReasoningEffortLevel::Low).unwrap(),
            json!("low")
        );
        assert_eq!(
            serde_json::to_value(ReasoningEffortLevel::Medium).unwrap(),
            json!("medium")
        );
        assert_eq!(
            serde_json::to_value(ReasoningEffortLevel::High).unwrap(),
            json!("high")
        );
    }

    #[test]
    fn test_reasoning_effort_level_deserialization() {
        let low: ReasoningEffortLevel = serde_json::from_value(json!("low")).unwrap();
        assert_eq!(low, ReasoningEffortLevel::Low);

        let medium: ReasoningEffortLevel = serde_json::from_value(json!("medium")).unwrap();
        assert_eq!(medium, ReasoningEffortLevel::Medium);

        let high: ReasoningEffortLevel = serde_json::from_value(json!("high")).unwrap();
        assert_eq!(high, ReasoningEffortLevel::High);
    }

    #[test]
    fn test_reasoning_effort_level_invalid_deserialization() {
        let result: Result<ReasoningEffortLevel, _> = serde_json::from_value(json!("invalid"));
        assert!(result.is_err());
    }

    #[test]
    fn test_reasoning_effort_level_display() {
        assert_eq!(ReasoningEffortLevel::Low.to_string(), "low");
        assert_eq!(ReasoningEffortLevel::Medium.to_string(), "medium");
        assert_eq!(ReasoningEffortLevel::High.to_string(), "high");
    }

    #[test]
    fn test_reasoning_effort_level_as_str() {
        assert_eq!(ReasoningEffortLevel::Low.as_str(), "low");
        assert_eq!(ReasoningEffortLevel::Medium.as_str(), "medium");
        assert_eq!(ReasoningEffortLevel::High.as_str(), "high");
    }

    #[test]
    fn test_reasoning_effort_level_in_struct() {
        #[derive(Serialize, Deserialize, Debug)]
        struct TestStruct {
            reasoning_effort: ReasoningEffortLevel,
        }

        let json = json!({
            "reasoning_effort": "medium"
        });
        let test_struct: Result<TestStruct, _> = serde_json::from_value(json);
        assert!(test_struct.is_ok());
        assert_eq!(
            test_struct.unwrap().reasoning_effort,
            ReasoningEffortLevel::Medium
        );
    }
}