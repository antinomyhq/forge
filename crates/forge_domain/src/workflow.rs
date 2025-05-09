use std::collections::HashMap;

use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::temperature::Temperature;
use crate::{Agent, AgentId, ModelId};

/// Configuration for a workflow that contains all settings
/// required to initialize a workflow.
#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option)]
pub struct Workflow {
    /// Agents that are part of this workflow
    #[merge(strategy = crate::merge::vec::unify_by_key)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<Agent>,

    /// Variables that can be used in templates
    #[merge(strategy = crate::merge::hashmap)]
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, Value>,

    /// Commands that can be used to interact with the workflow
    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<Command>,

    /// Default model ID to use for agents in this workflow
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,

    /// Maximum depth to which the file walker should traverse for all agents
    /// If not provided, each agent's individual setting will be used
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_walker_depth: Option<usize>,

    /// A set of custom rules that all agents should follow
    /// These rules will be applied in addition to each agent's individual rules
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
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
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub temperature: Option<Temperature>,

    /// Flag to enable/disable tool support for all agents in this workflow.
    /// If not specified, each agent's individual setting will be used.
    /// Default is false (tools disabled) when not specified.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub tool_supported: Option<bool>,
}

impl Default for Workflow {
    fn default() -> Self {
        serde_yml::from_str(include_str!("../../../forge.default.yaml")).unwrap()
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Command {
    #[merge(strategy = crate::merge::std::overwrite)]
    pub name: String,

    #[merge(strategy = crate::merge::std::overwrite)]
    pub description: String,

    #[merge(strategy = crate::merge::option)]
    pub prompt: Option<String>,
}

impl Workflow {
    /// Creates a new empty workflow with all fields set to their empty state.
    /// This is useful for testing where you want to build a workflow from
    /// scratch.
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            variables: HashMap::new(),
            commands: Vec::new(),
            model: None,
            max_walker_depth: None,
            custom_rules: None,
            temperature: None,
            tool_supported: None,
        }
    }

    fn find_agent(&self, id: &AgentId) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == *id)
    }

    pub fn get_agent(&self, id: &AgentId) -> crate::Result<&Agent> {
        self.find_agent(id)
            .ok_or_else(|| crate::Error::AgentUndefined(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_workflow_new_creates_empty_workflow() {
        // Arrange

        // Act
        let actual = Workflow::new();

        // Assert
        assert!(actual.agents.is_empty());
        assert!(actual.variables.is_empty());
        assert!(actual.commands.is_empty());
        assert_eq!(actual.model, None);
        assert_eq!(actual.max_walker_depth, None);
        assert_eq!(actual.custom_rules, None);
        assert_eq!(actual.temperature, None);
        assert_eq!(actual.tool_supported, None);
    }

    #[test]
    fn test_workflow_with_tool_supported() {
        // Arrange
        let fixture = r#"
        {
            "tool_supported": true,
            "agents": [
                {
                    "id": "test-agent",
                    "description": "Test agent"
                }
            ]
        }
        "#;

        // Act
        let actual: Workflow = serde_json::from_str(fixture).unwrap();

        // Assert
        assert_eq!(actual.tool_supported, Some(true));
    }

    #[test]
    fn test_workflow_merge_tool_supported() {
        // Fixture
        let mut base = Workflow::new();

        let other = Workflow::new().tool_supported(true);

        // Act
        base.merge(other);

        // Assert
        assert_eq!(base.tool_supported, Some(true));
    }

    #[test]
    fn test_workflow_merge_tool_supported_with_existing() {
        // Fixture
        let mut base = Workflow::new().tool_supported(false);

        let other = Workflow::new().tool_supported(true);

        // Act
        base.merge(other);

        // Assert
        assert_eq!(base.tool_supported, Some(true));
    }
}
