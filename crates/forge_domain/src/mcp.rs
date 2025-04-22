use std::collections::HashMap;
use std::ops::Deref;

use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

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
#[serde(transparent)]
pub struct McpServers(HashMap<String, McpServerConfig>);

impl Deref for McpServers {
    type Target = HashMap<String, McpServerConfig>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<HashMap<String, McpServerConfig>> for McpServers {
    fn from(value: HashMap<String, McpServerConfig>) -> Self {
        Self(value)
    }
}
