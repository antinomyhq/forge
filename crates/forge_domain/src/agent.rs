use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

use crate::merge::Key;
use crate::template::Template;
use crate::{Environment, EventContext, ModelId, ToolName};

#[derive(Debug, Default, Setters, Clone, Serialize, Deserialize)]
#[setters(strip_option)]
pub struct SystemContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Environment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_information: Option<String>,
    /// Indicates whether the agent supports tools.
    /// This value is populated directly from the Agent configuration.
    #[serde(default)]
    pub tool_supported: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    pub readme: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project_rules: String,
}

#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);
impl AgentId {
    pub fn new(id: impl ToString) -> Self {
        Self(id.to_string())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<ToolName> for AgentId {
    fn from(value: ToolName) -> Self {
        Self(value.into_string())
    }
}

fn is_true(value: &bool) -> bool {
    *value
}

fn truth() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Merge)]
pub struct Agent {
    /// Flag to enable/disable tool support for this agent.
    #[serde(default)]
    #[merge(strategy = crate::merge::bool::overwrite_false)]
    pub tool_supported: bool,
    #[merge(strategy = crate::merge::std::overwrite)]
    pub id: AgentId,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<Template<SystemContext>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<Template<EventContext>>,

    /// When set to true all user events will also contain a suggestions field
    /// that is prefilled with the matching information from vector store.
    #[serde(skip_serializing_if = "is_true", default)]
    #[merge(strategy = crate::merge::bool::overwrite_false)]
    pub suggestions: bool,

    /// Suggests if the agent needs to maintain its state for the lifetime of
    /// the program.    
    #[serde(skip_serializing_if = "is_true", default)]
    #[merge(strategy = crate::merge::bool::overwrite_false)]
    pub ephemeral: bool,

    /// Flag to enable/disable the agent. When disabled (false), the agent will
    /// be completely ignored during orchestration execution.
    #[serde(skip_serializing_if = "is_true", default = "truth")]
    #[merge(strategy = crate::merge::bool::overwrite_false)]
    pub enable: bool,

    /// Tools that the agent can use    
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    #[merge(strategy = crate::merge::vec::unify)]
    pub tools: Vec<ToolName>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    #[merge(strategy = crate::merge::vec::append)]
    pub transforms: Vec<Transform>,

    /// Used to specify the events the agent is interested in    
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    #[merge(strategy = crate::merge::vec::unify)]
    pub subscribe: Vec<String>,

    /// Maximum number of turns the agent can take    
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_turns: Option<u64>,

    /// Maximum depth to which the file walker should traverse for this agent
    /// If not provided, the maximum possible depth will be used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_walker_depth: Option<usize>,

    /// Rules that the agent needs to follow.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    #[merge(strategy = crate::merge::string::concat)]
    pub project_rules: String,
}

impl Key for Agent {
    type Id = AgentId;

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
        input: String,
        output: String,
        agent_id: AgentId,
        token_limit: usize,
    },

    /// Works on the user prompt by enriching it with additional information
    User {
        agent_id: AgentId,
        output: String,
        input: String,
    },

    /// Intercepts the context and performs an operation without changing the
    /// context
    PassThrough { agent_id: AgentId, input: String },
}

#[cfg(test)]
mod tests {
    use merge::Merge;

    use super::*;

    impl Default for Agent {
        fn default() -> Self {
            Agent {
                tool_supported: false,
                id: AgentId(String::new()),
                model: None,
                description: None,
                system_prompt: None,
                user_prompt: None,
                suggestions: false,
                ephemeral: false,
                enable: true, // Assuming default is enabled
                tools: Vec::new(),
                transforms: Vec::new(),
                subscribe: Vec::new(),
                max_turns: None,
                max_walker_depth: None,
                project_rules: String::new(),
            }
        }
    }

    #[test]
    fn test_merge_project_rules() {
        // case 1: base has some project rules and other has some rules
        let mut base = Agent::default();
        base.project_rules = "Rule 1: Be concise".to_string();

        let other = Agent {
            project_rules: "Rule 2: Be precise".to_string(),
            ..Agent::default()
        };

        base.merge(other);
        assert_eq!(base.project_rules, "Rule 1: Be concise\nRule 2: Be precise");

        // case 2: base has empty project rules but other has some rules
        let mut base = Agent::default();
        let other = Agent {
            project_rules: "Rule 1: Be precise".to_string(),
            ..Agent::default()
        };

        base.merge(other);
        assert_eq!(base.project_rules, "Rule 1: Be precise");

        // case 3: base and other has empty project rules
        let mut base = Agent::default();

        let other = Agent::default();
        base.merge(other);
        assert!(base.project_rules.is_empty());

        // case 4: base has some project rules and other has no project rules.
        let mut base = Agent::default();
        base.project_rules = "Rule 1: Be concise".to_string();

        let other = Agent::default();
        base.merge(other);
        assert_eq!(base.project_rules, "Rule 1: Be concise");
    }
}
