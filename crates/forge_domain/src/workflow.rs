use std::collections::HashMap;

use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Agent, AgentId};

// Model Context Protocol (MCP) related types

/// MCP client configuration
#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    /// MCP HTTP servers
    #[merge(strategy = crate::merge::option)]
    pub http: Option<HashMap<String, McpHttpServerConfig>>,
    
    /// MCP servers
    #[merge(strategy = crate::merge::option)]
    pub fs: Option<HashMap<String, McpFsServerConfig>>,
    
    // /// Filesystem roots that client can access
    // #[merge(strategy = crate::merge::vec::append)]
    // #[serde(default)]
    // pub roots: Vec<McpRoot>,
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

/*
TODO: impl this later
/// Filesystem root that can be accessed by MCP servers
#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge)]
pub struct McpRoot {
    /// URI of the filesystem root (file:// URI)
    #[merge(strategy = crate::merge::std::overwrite)]
    pub uri: String,
    
    /// Display name for the root
    #[merge(strategy = crate::merge::option)]
    pub name: Option<String>,
}*/

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option)]
pub struct Workflow {
    #[merge(strategy = crate::merge::vec::unify_by_key)]
    pub agents: Vec<Agent>,

    #[merge(strategy = crate::merge::option)]
    pub variables: Option<HashMap<String, Value>>,

    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default)]
    pub commands: Vec<Command>,
    
    /// Model Context Protocol (MCP) configuration
    #[merge(strategy = crate::merge::option)]
    pub mcp: Option<McpConfig>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Command {
    #[merge(strategy = crate::merge::std::overwrite)]
    pub name: String,

    #[merge(strategy = crate::merge::std::overwrite)]
    pub description: String,

    #[merge(strategy = crate::merge::option)]
    pub value: Option<String>,
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