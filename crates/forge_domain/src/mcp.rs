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

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters, PartialEq, Hash)]
#[setters(strip_option, into)]
pub struct McpServer {
    /// Command to execute for starting this MCP server
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments to pass to the command
    #[merge(strategy = crate::merge::vec::append)]
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables to pass to the command
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,

    /// Url of the MCP server
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Display for McpServer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        if let Some(command) = self.command.as_ref() {
            output.push_str(&format!("{command} "));
            self.args.iter().for_each(|arg| {
                output.push_str(&format!("{arg} "));
            });

            if let Some(env) = self.env.as_ref() {
                env.iter().for_each(|(key, value)| {
                    output.push_str(&format!("{key}={value} "));
                });
            }
        }

        if let Some(url) = self.url.as_ref() {
            output.push_str(&format!("{url} "));
        }

        write!(f, "{}", output.trim())
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    pub mcp_servers: BTreeMap<String, McpServer>,
}

impl Deref for McpConfig {
    type Target = BTreeMap<String, McpServer>;

    fn deref(&self) -> &Self::Target {
        &self.mcp_servers
    }
}

impl From<BTreeMap<String, McpServer>> for McpConfig {
    fn from(mcp_servers: BTreeMap<String, McpServer>) -> Self {
        Self { mcp_servers }
    }
}
