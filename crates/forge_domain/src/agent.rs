use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Deserializer, Serialize};

use crate::merge::Key;
use crate::template::Template;
use crate::{Error, EventContext, ModelId, Result, SystemContext, ToolDefinition, ToolName};

// Unique identifier for an agent
#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);
impl AgentId {
    // Creates a new agent ID from a string-like value
    pub fn new(id: impl ToString) -> Self {
        Self(id.to_string())
    }

    // Returns the agent ID as a string reference
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<ToolName> for AgentId {
    // Converts a ToolName into an AgentId
    fn from(value: ToolName) -> Self {
        Self(value.into_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Agent {
    /// Controls whether this agent's output should be hidden from the console
    /// When false (default), output is not displayed
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub hide_content: Option<bool>,

    /// Flag to disable this agent, when true agent will not be activated
    /// Default is false (agent is enabled)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub disable: Option<bool>,

    /// Flag to enable/disable tool support for this agent.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub tool_supported: Option<bool>,

    // Unique identifier for the agent
    #[merge(strategy = crate::merge::std::overwrite)]
    pub id: AgentId,

    // The language model ID to be used by this agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub model: Option<ModelId>,

    // Human-readable description of the agent's purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub description: Option<String>,

    // Template for the system prompt provided to the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub system_prompt: Option<Template<SystemContext>>,

    // Template for the user prompt provided to the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub user_prompt: Option<Template<EventContext>>,

    /// When set to true all user events will also contain a suggestions field
    /// that is prefilled with the matching information from vector store.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub suggestions: Option<bool>,

    /// Suggests if the agent needs to maintain its state for the lifetime of
    /// the program.    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub ephemeral: Option<bool>,

    /// Tools that the agent can use    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub tools: Option<Vec<ToolName>>,

    // Transformations to be applied to the agent's context
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub transforms: Option<Vec<Transform>>,

    /// Used to specify the events the agent is interested in    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = merge_subscription)]
    pub subscribe: Option<Vec<String>>,

    /// Maximum number of turns the agent can take    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_turns: Option<u64>,

    /// Maximum depth to which the file walker should traverse for this agent
    /// If not provided, the maximum possible depth will be used
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_walker_depth: Option<usize>,

    /// A set of custom rules that the agent should follow
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
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
    #[serde(deserialize_with = "validate_temperature_range")]
    #[merge(strategy = crate::merge::option)]
    pub temperature: Option<f32>,
}

// validate temperature range during deserialization
fn validate_temperature_range<'de, D>(deserializer: D) -> std::result::Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // Deserialize as Option<f32>
    let opt = Option::<f32>::deserialize(deserializer)?;

    // If Some value, validate the range
    if let Some(temp) = opt {
        if !(0.0..=2.0).contains(&temp) {
            return Err(Error::custom(format!(
                "temperature must be between 0.0 and 2.0, got {}",
                temp
            )));
        }
    }

    Ok(opt)
}

fn merge_subscription(base: &mut Option<Vec<String>>, other: Option<Vec<String>>) {
    if let Some(other) = other {
        if let Some(base) = base {
            base.extend(other);
        } else {
            *base = Some(other);
        }
    }
}

impl Agent {
    pub fn new(id: impl ToString) -> Self {
        Self {
            id: AgentId::new(id),
            disable: None,
            tool_supported: None,
            model: None,
            description: None,
            system_prompt: None,
            user_prompt: None,
            suggestions: None,
            ephemeral: None,
            tools: None,
            transforms: None,
            subscribe: None,
            max_turns: None,
            max_walker_depth: None,
            custom_rules: None,
            hide_content: None,
            temperature: None,
        }
    }

    pub fn tool_definition(&self) -> Result<ToolDefinition> {
        if self.description.is_none() || self.description.as_ref().is_none_or(|d| d.is_empty()) {
            return Err(Error::MissingAgentDescription(self.id.clone()));
        }
        Ok(ToolDefinition::new(self.id.as_str().to_string())
            .description(self.description.clone().unwrap()))
    }
}

impl Key for Agent {
    // Define the ID type for the Key trait implementation
    type Id = AgentId;

    // Return a reference to the agent's ID
    fn key(&self) -> &Self::Id {
        &self.id
    }
}

/// Transformations that can be applied to the agent's context before sending it
/// upstream to the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transform {
    /// Compresses multiple assistant messages into a single message
    Assistant {
        // Input template for the transformation
        input: String,
        // Output template after transformation
        output: String,
        // ID of the agent performing the transformation
        agent_id: AgentId,
        // Maximum token limit for the compressed message
        token_limit: usize,
    },

    /// Works on the user prompt by enriching it with additional information
    User {
        // ID of the agent performing the transformation
        agent_id: AgentId,
        // Output template after transformation
        output: String,
        // Input template for the transformation
        input: String,
    },

    /// Intercepts the context and performs an operation without changing the
    /// context
    PassThrough {
        // ID of the agent performing the pass-through
        agent_id: AgentId,
        // Input template for the transformation
        input: String,
    },
}

#[cfg(test)]
mod hide_content_tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_merge_hide_content() {
        // Base has no value, other has value
        let mut base = Agent::new("Base"); // No hide_content set
        let other = Agent::new("Other").hide_content(true);
        base.merge(other);
        assert_eq!(base.hide_content, Some(true));

        // Base has a value, other has another value
        let mut base = Agent::new("Base").hide_content(false);
        let other = Agent::new("Other").hide_content(true);
        base.merge(other);
        assert_eq!(base.hide_content, Some(true));
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_merge_model() {
        // Base has a value, should not be overwritten
        let mut base = Agent::new("Base").model(ModelId::new("base"));
        let other = Agent::new("Other").model(ModelId::new("other"));
        base.merge(other);
        assert_eq!(base.model.unwrap(), ModelId::new("other"));

        // Base has no value, should take the other value
        let mut base = Agent::new("Base"); // No model
        let other = Agent::new("Other").model(ModelId::new("other"));
        base.merge(other);
        assert_eq!(base.model.unwrap(), ModelId::new("other"));
    }

    #[test]
    fn test_merge_tool_supported() {
        // Base has no value, should use other's value
        let mut base = Agent::new("Base"); // No tool_supported set
        let other = Agent::new("Other").tool_supported(true);
        base.merge(other);
        assert_eq!(base.tool_supported, Some(true));

        // Base has a value, should not be overwritten
        let mut base = Agent::new("Base").tool_supported(false);
        let other = Agent::new("Other").tool_supported(true);
        base.merge(other);
        assert_eq!(base.tool_supported, Some(true));
    }

    #[test]
    fn test_merge_disable() {
        // Base has no value, should use other's value
        let mut base = Agent::new("Base"); // No disable set
        let other = Agent::new("Other").disable(true);
        base.merge(other);
        assert_eq!(base.disable, Some(true));

        // Base has a value, should be overwritten
        let mut base = Agent::new("Base").disable(false);
        let other = Agent::new("Other").disable(true);
        base.merge(other);
        assert_eq!(base.disable, Some(true));
    }

    #[test]
    fn test_merge_bool_flags() {
        // With the option strategy, the first value is preserved
        let mut base = Agent::new("Base").suggestions(true);
        let other = Agent::new("Other").suggestions(false);
        base.merge(other);
        assert_eq!(base.suggestions, Some(false));

        // Now test with no initial value
        let mut base = Agent::new("Base"); // no suggestions set
        let other = Agent::new("Other").suggestions(false);
        base.merge(other);
        assert_eq!(base.suggestions, Some(false));

        // Test ephemeral flag with option strategy
        let mut base = Agent::new("Base").ephemeral(true);
        let other = Agent::new("Other").ephemeral(false);
        base.merge(other);
        assert_eq!(base.ephemeral, Some(false));
    }

    #[test]
    fn test_merge_tools() {
        // Base has no value, should take other's values
        let mut base = Agent::new("Base"); // no tools
        let other = Agent::new("Other").tools(vec![ToolName::new("tool2"), ToolName::new("tool3")]);
        base.merge(other);

        // Should contain all tools from the other agent
        let tools = base.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&ToolName::new("tool2")));
        assert!(tools.contains(&ToolName::new("tool3")));

        // Base has a value, should not be overwritten
        let mut base =
            Agent::new("Base").tools(vec![ToolName::new("tool1"), ToolName::new("tool2")]);
        let other = Agent::new("Other").tools(vec![ToolName::new("tool3"), ToolName::new("tool4")]);
        base.merge(other);

        // Should have other's tools
        let tools = base.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&ToolName::new("tool3")));
        assert!(tools.contains(&ToolName::new("tool4")));
    }

    #[test]
    fn test_merge_transforms() {
        // Base has no value, should take other's values
        let mut base = Agent::new("Base"); // no transforms
        let transform2 = Transform::PassThrough {
            agent_id: AgentId::new("agent2"),
            input: "input2".to_string(),
        };
        let other = Agent::new("Other").transforms(vec![transform2]);

        base.merge(other);

        // Should contain transforms from the other agent
        let transforms = base.transforms.as_ref().unwrap();
        assert_eq!(transforms.len(), 1);
        if let Transform::PassThrough { agent_id, input } = &transforms[0] {
            assert_eq!(agent_id.as_str(), "agent2");
            assert_eq!(input, "input2");
        } else {
            panic!("Expected PassThrough transform");
        }

        // Base has a value, should not be overwritten
        let transform1 = Transform::PassThrough {
            agent_id: AgentId::new("agent1"),
            input: "input1".to_string(),
        };
        let mut base = Agent::new("Base").transforms(vec![transform1]);

        let transform2 = Transform::PassThrough {
            agent_id: AgentId::new("agent2"),
            input: "input2".to_string(),
        };
        let other = Agent::new("Other").transforms(vec![transform2]);

        base.merge(other);

        // Should have other's transforms
        let transforms = base.transforms.as_ref().unwrap();
        assert_eq!(transforms.len(), 1);
        if let Transform::PassThrough { agent_id, input } = &transforms[0] {
            assert_eq!(agent_id.as_str(), "agent2");
            assert_eq!(input, "input2");
        } else {
            panic!("Expected PassThrough transform");
        }
    }

    #[test]
    fn test_merge_subscribe() {
        // Base has no value, should take other's values
        let mut base = Agent::new("Base"); // no subscribe
        let other = Agent::new("Other").subscribe(vec!["event2".to_string(), "event3".to_string()]);
        base.merge(other);

        // Should contain events from other
        let subscribe = base.subscribe.as_ref().unwrap();
        assert_eq!(subscribe.len(), 2);
        assert!(subscribe.contains(&"event2".to_string()));
        assert!(subscribe.contains(&"event3".to_string()));

        // Base has a value, should not be overwritten
        let mut base =
            Agent::new("Base").subscribe(vec!["event1".to_string(), "event2".to_string()]);
        let other = Agent::new("Other").subscribe(vec!["event3".to_string(), "event4".to_string()]);
        base.merge(other);

        // Should have other's events
        let subscribe = base.subscribe.as_ref().unwrap();
        assert_eq!(subscribe.len(), 4);
        assert!(subscribe.contains(&"event1".to_string()));
        assert!(subscribe.contains(&"event2".to_string()));
        assert!(subscribe.contains(&"event3".to_string()));
        assert!(subscribe.contains(&"event4".to_string()));
    }

    #[test]
    fn test_temperature_validation() {
        // Valid temperature values should deserialize correctly
        let valid_temps = [0.0, 0.5, 1.0, 1.5, 2.0];
        for temp in valid_temps {
            let json = json!({
                "id": "test-agent",
                "temperature": temp
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(
                agent.is_ok(),
                "Valid temperature {} should deserialize",
                temp
            );
            assert_eq!(agent.unwrap().temperature, Some(temp));
        }

        // Invalid temperature values should fail deserialization
        let invalid_temps = [-0.1, 2.1, 3.0, -1.0, 10.0];
        for temp in invalid_temps {
            let json = json!({
                "id": "test-agent",
                "temperature": temp
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(
                agent.is_err(),
                "Invalid temperature {} should fail deserialization",
                temp
            );
            let err = agent.unwrap_err().to_string();
            assert!(
                err.contains("temperature must be between 0.0 and 2.0"),
                "Error should mention valid range: {}",
                err
            );
        }

        // No temperature should deserialize to None
        let json = json!({
            "id": "test-agent"
        });

        let agent: Agent = serde_json::from_value(json).unwrap();
        assert_eq!(agent.temperature, None);
    }
}
