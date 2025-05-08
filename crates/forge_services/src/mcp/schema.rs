use std::collections::HashMap;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct McpServerConfig {
    /// Command to execute for starting this MCP server
    pub command: Option<String>,

    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to pass to the command
    pub env: Option<HashMap<String, String>>,

    /// Url of the MCP server
    pub url: Option<String>,
}


#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServers {
    /// Map of server names to their configurations
    pub mcp_servers: HashMap<String, McpServerConfig>,
}