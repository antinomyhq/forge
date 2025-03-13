use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::Environment;

#[derive(Debug, Default, Setters, Clone, Serialize, Deserialize)]
#[setters(strip_option)]
pub struct SystemContext {
    // Environment information to be included in the system context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Environment>,

    // Information about available tools that can be used by the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_information: Option<String>,

    /// Indicates whether the agent supports tools.
    /// This value is populated directly from the Agent configuration.
    #[serde(default)]
    pub tool_supported: bool,

    // List of file paths that are relevant for the agent context
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    // README content to provide project context to the agent
    pub readme: String,

    // Project rules to be followed by the agent
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project_rules: String,

    /// Repository content indexed in memory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_content: Option<String>,
}
