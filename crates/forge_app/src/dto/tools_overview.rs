use std::collections::HashMap;

use derive_setters::Setters;
use forge_domain::ToolDefinition;
use serde::{Deserialize, Serialize};

/// A comprehensive view of all tools available in the environment,
/// categorized by their source type for easier navigation and understanding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct ToolsOverview {
    /// System tools provided by the Forge environment
    pub system: Vec<ToolDefinition>,
    /// Tools provided by registered agents
    pub agents: Vec<ToolDefinition>,
    /// Tools provided by MCP servers, grouped by server name
    pub mcp: HashMap<String, Vec<ToolDefinition>>,
}

impl ToolsOverview {
    /// Create a new empty ToolsOverview
    pub fn new() -> Self {
        ToolsOverview { system: Vec::new(), agents: Vec::new(), mcp: HashMap::new() }
    }

    // Creates a flat list of all tool definitions
    pub fn as_vec(&self) -> Vec<&ToolDefinition> {
        let mut tools = Vec::new();
        tools.extend(&self.system);
        tools.extend(&self.agents);
        for server_tools in self.mcp.values() {
            tools.extend(server_tools);
        }
        tools
    }
}

impl Default for ToolsOverview {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ToolsOverview> for Vec<ToolDefinition> {
    fn from(value: ToolsOverview) -> Self {
        value.as_vec().into_iter().cloned().collect()
    }
}
