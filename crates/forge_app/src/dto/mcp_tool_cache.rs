use std::collections::BTreeMap;

use forge_domain::ToolDefinition;
use serde::{Deserialize, Serialize};

/// Cache for MCP tool definitions
///
/// Simplified cache structure that stores only the essential data.
/// Validation and TTL checking are handled by the infrastructure layer
/// using cacache's built-in metadata capabilities.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCache {
    pub config_hash: String,
    /// Tools mapped by server name
    pub tools: BTreeMap<String, Vec<ToolDefinition>>,
}

impl McpToolCache {
    /// Create a new cache entry
    pub fn new(config_hash: String, tools: BTreeMap<String, Vec<ToolDefinition>>) -> Self {
        Self { config_hash, tools }
    }
}
