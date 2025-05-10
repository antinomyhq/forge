use std::collections::HashMap;
use std::ops::Deref;

use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Local,
    User,
    Project,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct McpServerConfig {
    /// Command to execute for starting this MCP server
    #[merge(strategy = crate::merge::option)]
    pub command: Option<String>,

    /// Arguments to pass to the command
    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to pass to the command
    #[merge(strategy = crate::merge::option)]
    pub env: Option<HashMap<String, String>>,

    /// Url of the MCP server
    #[merge(strategy = crate::merge::option)]
    pub url: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServers {
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

impl Deref for McpServers {
    type Target = HashMap<String, McpServerConfig>;

    fn deref(&self) -> &Self::Target {
        &self.mcp_servers
    }
}

impl From<HashMap<String, McpServerConfig>> for McpServers {
    fn from(mcp_servers: HashMap<String, McpServerConfig>) -> Self {
        Self { mcp_servers }
    }
}
