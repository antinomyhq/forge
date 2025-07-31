//!
//! Follows the design specifications of Claude's [.mcp.json](https://docs.anthropic.com/en/docs/claude-code/tutorials#set-up-model-context-protocol-mcp)

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Local,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
#[serde(untagged)]
pub enum McpServerConfig {
    Stdio(McpStdioServer),
    Sse(McpSseServer),
}

impl McpServerConfig {
    /// Create a new stdio-based MCP server
    pub fn new_stdio(
        command: impl Into<String>,
        args: Vec<String>,
        env: Option<BTreeMap<String, String>>,
    ) -> Self {
        Self::Stdio(McpStdioServer { command: command.into(), args, env: env.unwrap_or_default() })
    }

    /// Create a new SSE-based MCP server
    pub fn new_sse(url: impl Into<String>) -> Self {
        Self::Sse(McpSseServer { url: url.into() })
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Setters, PartialEq, Hash)]
#[setters(strip_option, into)]
pub struct McpStdioServer {
    /// Command to execute for starting this MCP server
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,

    /// Arguments to pass to the command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables to pass to the command
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct McpSseServer {
    /// Url of the MCP server
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
}

impl Display for McpServerConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        match self {
            McpServerConfig::Stdio(stdio) => {
                output.push_str(&format!("{} ", stdio.command));
                stdio.args.iter().for_each(|arg| {
                    output.push_str(&format!("{arg} "));
                });

                stdio.env.iter().for_each(|(key, value)| {
                    output.push_str(&format!("{key}={value} "));
                });
            }
            McpServerConfig::Sse(sse) => {
                output.push_str(&format!("{} ", sse.url));
            }
        }

        write!(f, "{}", output.trim())
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Merge)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    #[serde(default)]
    #[merge(strategy = std::collections::BTreeMap::extend)]
    pub mcp_servers: BTreeMap<String, McpServerConfig>,
}

impl Deref for McpConfig {
    type Target = BTreeMap<String, McpServerConfig>;

    fn deref(&self) -> &Self::Target {
        &self.mcp_servers
    }
}

impl From<BTreeMap<String, McpServerConfig>> for McpConfig {
    fn from(mcp_servers: BTreeMap<String, McpServerConfig>) -> Self {
        Self { mcp_servers }
    }
}
