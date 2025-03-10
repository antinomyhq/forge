use derive_more::derive::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Flag to enable/disable tool support for this agent.
    #[serde(default)]
    pub tool_supported: bool,
    pub id: AgentId,
    pub model: ModelId,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<Template<SystemContext>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<Template<EventContext>>,

    /// When set to true all user events will also contain a suggestions field
    /// that is prefilled with the matching information from vector store.
    #[serde(skip_serializing_if = "is_true", default)]
    pub suggestions: bool,

    /// Suggests if the agent needs to maintain its state for the lifetime of
    /// the program.    
    #[serde(skip_serializing_if = "is_true", default = "truth")]
    pub ephemeral: bool,

    /// Flag to enable/disable the agent. When disabled (false), the agent will
    /// be completely ignored during orchestration execution.
    #[serde(skip_serializing_if = "is_true", default = "truth")]
    pub enable: bool,

    /// Tools that the agent can use    
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolName>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub transforms: Vec<Transform>,

    /// Used to specify the events the agent is interested in    
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub subscribe: Vec<String>,

    /// Maximum number of turns the agent can take    
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_turns: Option<u64>,

    /// Maximum depth to which the file walker should traverse for this agent
    /// If not provided, the maximum possible depth will be used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_walker_depth: Option<usize>,
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
