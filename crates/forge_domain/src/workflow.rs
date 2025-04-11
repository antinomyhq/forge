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
#[setters(strip_option, into)]
pub struct Workflow {
    /// Agents that are part of this workflow
    #[merge(strategy = crate::merge::vec::unify_by_key)]
    pub agents: Vec<Agent>,

    /// Variables that can be used in templates
    #[merge(strategy = crate::merge::hashmap)]
    #[serde(default)]
    pub variables: HashMap<String, Value>,

    /// Commands that can be used to interact with the workflow
    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default)]
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

    /// Model Context Protocol (MCP) configuration
    #[merge(strategy = crate::merge::option)]
    pub mcp: Option<McpConfig>,
}

/// MCP client configuration
#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
pub struct McpConfig {
    /// MCP HTTP servers
    #[merge(strategy = crate::merge::option)]
    pub http: Option<HashMap<String, McpHttpServerConfig>>,

    /// MCP servers
    #[merge(strategy = crate::merge::option)]
    pub fs: Option<HashMap<String, McpFsServerConfig>>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge)]
pub struct McpFsServerConfig {
    /// Command to execute for starting this MCP server
    #[merge(strategy = crate::merge::std::overwrite)]
    pub command: String,

    /// Arguments to pass to the command
    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to pass to the command
    #[merge(strategy = crate::merge::option)]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge)]
pub struct McpHttpServerConfig {
    /// Url of the MCP server
    #[merge(strategy = crate::merge::std::overwrite)]
    pub url: String,
}

impl Default for Workflow {
    fn default() -> Self {
        serde_yaml::from_str(include_str!("../../../forge.default.yaml")).unwrap()
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
    fn find_agent(&self, id: &AgentId) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == *id)
    }

    pub fn get_agent(&self, id: &AgentId) -> crate::Result<&Agent> {
        self.find_agent(id)
            .ok_or_else(|| crate::Error::AgentUndefined(id.clone()))
    }
}
