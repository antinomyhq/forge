use std::collections::{HashMap, hash_map};
use std::ops::Deref;

use derive_more::From;
use serde::{Deserialize, Serialize};

use crate::{ServerName, ToolDefinition};

/// Cache for MCP tool definitions
///
/// Simplified cache structure that stores only the essential data.
/// Validation and TTL checking are handled by the infrastructure layer
/// using cacache's built-in metadata capabilities.
#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq, From)]
#[serde(rename_all = "camelCase")]
pub struct McpServers(HashMap<ServerName, Vec<ToolDefinition>>);

impl Deref for McpServers {
    type Target = HashMap<ServerName, Vec<ToolDefinition>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl McpServers {
    /// Create a new cache entry
    pub fn new(tools: HashMap<ServerName, Vec<ToolDefinition>>) -> Self {
        Self(tools)
    }
}

impl IntoIterator for McpServers {
    type Item = (ServerName, Vec<ToolDefinition>);
    type IntoIter = hash_map::IntoIter<ServerName, Vec<ToolDefinition>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
